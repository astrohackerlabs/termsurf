//! Kitty graphics command execution.

use super::super::point::{Coordinate, Point};
use super::super::screen::Screen;
use super::graphics_command::{
    AnimationControl, AnimationFrameComposition, AnimationFrameLoading, Command, CommandControl,
    CursorMovement, Delete, Display, Quiet, Response, Transmission,
};
use super::graphics_image::{ImageLoadError, LoadingImage};
use super::graphics_storage::{
    CellMetrics, ImageStorage, Placement, PlacementError, PlacementId, PlacementKey,
    PlacementLocation,
};

pub(crate) fn execute(storage: &mut ImageStorage, command: &Command) -> Option<Response<'static>> {
    if !storage.enabled() {
        return None;
    }

    let mut quiet = command.quiet;
    let response = match &command.control {
        CommandControl::Query(transmission) => query(storage, command, *transmission),
        CommandControl::Transmit(_) => {
            if let Some(loading) = storage.loading.as_mut() {
                match command.quiet {
                    Quiet::No => quiet = loading.quiet,
                    Quiet::Ok | Quiet::Failures => loading.quiet = command.quiet,
                }
            }
            Some(transmit(storage, command))
        }
        CommandControl::TransmitAndDisplay { transmission, .. } => {
            Some(unimplemented_response_for_transmission(*transmission))
        }
        CommandControl::Display(display) => Some(unimplemented_response_for_display(*display)),
        CommandControl::Delete(delete) => Some(unimplemented_response_for_delete(*delete)),
        CommandControl::TransmitAnimationFrame(animation) => {
            Some(unimplemented_response_for_animation_frame(*animation))
        }
        CommandControl::ControlAnimation(animation) => {
            Some(unimplemented_response_for_animation_control(*animation))
        }
        CommandControl::ComposeAnimation(composition) => Some(
            unimplemented_response_for_animation_composition(*composition),
        ),
    };

    filter_response(response, quiet)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::terminal) struct ScreenExecution {
    pub(in crate::terminal) response: Option<Response<'static>>,
    pub(in crate::terminal) cursor_after: Option<CursorAfter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::terminal) struct CursorAfter {
    pub(in crate::terminal) pin_x: u32,
    pub(in crate::terminal) columns: u32,
    pub(in crate::terminal) rows: u32,
}

pub(in crate::terminal) fn execute_screen(
    screen: &mut Screen,
    command: &Command,
    metrics: CellMetrics,
) -> ScreenExecution {
    if !screen.kitty_images().enabled() {
        return ScreenExecution::default();
    }

    let mut quiet = command.quiet;
    if matches!(
        &command.control,
        CommandControl::Transmit(_) | CommandControl::TransmitAndDisplay { .. }
    ) {
        if let Some(loading) = screen.kitty_images_mut().loading.as_mut() {
            match command.quiet {
                Quiet::No => quiet = loading.quiet,
                Quiet::Ok | Quiet::Failures => loading.quiet = command.quiet,
            }
        }
    }

    let mut cursor_after = None;
    let response = match &command.control {
        CommandControl::Query(transmission) => query(screen.kitty_images(), command, *transmission),
        CommandControl::Transmit(_) => {
            let (response, _) = transmit_screen(screen, command, metrics);
            Some(response)
        }
        CommandControl::TransmitAndDisplay { .. } => {
            let (result, after) = transmit_screen(screen, command, metrics);
            cursor_after = after;
            Some(result)
        }
        CommandControl::Display(display) => {
            let result = display_on_screen(screen, *display);
            if result.ok() {
                cursor_after = cursor_after_for_display(screen, *display, metrics);
            }
            Some(result)
        }
        CommandControl::Delete(delete) => {
            screen.delete_kitty(*delete, metrics);
            None
        }
        CommandControl::TransmitAnimationFrame(animation) => {
            Some(unimplemented_response_for_animation_frame(*animation))
        }
        CommandControl::ControlAnimation(animation) => {
            Some(unimplemented_response_for_animation_control(*animation))
        }
        CommandControl::ComposeAnimation(composition) => Some(
            unimplemented_response_for_animation_composition(*composition),
        ),
    };

    ScreenExecution {
        response: filter_response(response, quiet),
        cursor_after,
    }
}

fn query(
    storage: &ImageStorage,
    command: &Command,
    transmission: Transmission,
) -> Option<Response<'static>> {
    if transmission.image_id == 0 {
        return None;
    }

    let mut response = response_for_transmission(transmission);
    if let Err(err) = LoadingImage::init(command, storage.image_limits) {
        encode_error(&mut response, err);
    }

    Some(response)
}

fn transmit(storage: &mut ImageStorage, command: &Command) -> Response<'static> {
    let Some(transmission) = command.transmission() else {
        return error_response(b"EINVAL: invalid data");
    };
    let mut response = response_for_transmission(transmission);
    if let Some(loading) = storage.loading.as_ref() {
        response.id = loading.image.id;
        response.image_number = loading.image.number;
    }

    if transmission.image_id > 0 && transmission.image_number > 0 {
        response.message = b"EINVAL: image ID and number are mutually exclusive";
        return response;
    }

    match load_and_add_image(storage, command, transmission) {
        Ok(load) => {
            if load.more || load.implicit_id {
                return Response::default();
            }

            response.id = load.id;
            response.image_number = load.image_number;
            response.placement_id = load.placement_id;
            response
        }
        Err(err) => {
            encode_error(&mut response, err);
            response
        }
    }
}

fn transmit_screen(
    screen: &mut Screen,
    command: &Command,
    metrics: CellMetrics,
) -> (Response<'static>, Option<CursorAfter>) {
    let Some(transmission) = command.transmission() else {
        return (error_response(b"EINVAL: invalid data"), None);
    };
    let mut response = response_for_transmission(transmission);
    if let Some(loading) = screen.kitty_images().loading.as_ref() {
        response.id = loading.image.id;
        response.image_number = loading.image.number;
    }

    if transmission.image_id > 0 && transmission.image_number > 0 {
        response.message = b"EINVAL: image ID and number are mutually exclusive";
        return (response, None);
    }

    match load_and_add_image_screen(screen, command, transmission) {
        Ok(load) => {
            let mut cursor_after = None;
            let mut response = if !load.more {
                if let Some(mut display) = load.display {
                    display.image_id = load.id;
                    let response = display_on_screen(screen, display);
                    if response.ok() {
                        cursor_after = cursor_after_for_display(screen, display, metrics);
                    }
                    response
                } else {
                    response
                }
            } else {
                response
            };

            if load.more || load.implicit_id {
                return (Response::default(), cursor_after);
            }

            response.id = load.id;
            response.image_number = load.image_number;
            (response, cursor_after)
        }
        Err(err) => {
            encode_error(&mut response, err);
            (response, None)
        }
    }
}

fn cursor_after_for_display(
    screen: &Screen,
    display: Display,
    metrics: CellMetrics,
) -> Option<CursorAfter> {
    if display.cursor_movement != CursorMovement::After || display.virtual_placement {
        return None;
    }

    let image_id = if display.image_id != 0 {
        display.image_id
    } else {
        screen
            .kitty_images()
            .image_by_number(display.image_number)?
            .id
    };
    let placement_id = if display.placement_id == 0 {
        PlacementId::Internal(
            screen
                .kitty_images()
                .next_internal_placement_id
                .wrapping_sub(1),
        )
    } else {
        PlacementId::External(display.placement_id)
    };
    let key = PlacementKey {
        image_id,
        placement_id,
    };
    let placement = screen.kitty_images().placement_by_key(key)?;
    let pin = placement.tracked_pin()?;
    let pin_x = screen.tracked_pin_value(pin)?.x();
    let image = screen.kitty_images().image_by_id(image_id)?;
    let grid = placement.grid_size(image, metrics);
    Some(CursorAfter {
        pin_x: pin_x.into(),
        columns: grid.columns,
        rows: grid.rows,
    })
}

fn display_on_screen(screen: &mut Screen, display: Display) -> Response<'static> {
    if display.image_id == 0 && display.image_number == 0 {
        return error_response(b"EINVAL: image ID or number required");
    }

    let mut response = Response {
        id: display.image_id,
        image_number: display.image_number,
        placement_id: display.placement_id,
        message: b"OK",
    };

    let image_id = if display.image_id != 0 {
        match screen.kitty_images().image_by_id(display.image_id) {
            Some(image) => image.id,
            None => {
                response.message = b"ENOENT: image not found";
                return response;
            }
        }
    } else {
        match screen.kitty_images().image_by_number(display.image_number) {
            Some(image) => image.id,
            None => {
                response.message = b"ENOENT: image not found";
                return response;
            }
        }
    };
    response.id = image_id;

    let location = if display.virtual_placement {
        if display.parent_id > 0 {
            response.message = b"EINVAL: virtual placement cannot refer to a parent";
            return response;
        }
        PlacementLocation::Virtual
    } else {
        let (x, y) = screen.cursor_position();
        let Some(pin) = screen.pin(Point::active(Coordinate::new(x, y.into()))) else {
            response.message = b"EINVAL: failed to prepare terminal state";
            return response;
        };
        let Some(pin) = screen.track_pin(pin) else {
            response.message = b"EINVAL: failed to prepare terminal state";
            return response;
        };
        PlacementLocation::Pin(pin)
    };

    let placement = Placement {
        location,
        x_offset: display.x_offset,
        y_offset: display.y_offset,
        source_x: display.x,
        source_y: display.y,
        source_width: display.width,
        source_height: display.height,
        columns: display.columns,
        rows: display.rows,
        z: display.z,
    };

    if let Err(err) = screen.add_kitty_placement(image_id, display.placement_id, placement) {
        encode_placement_error(&mut response, err);
    }

    response
}

pub(crate) fn display_with_location(
    storage: &mut ImageStorage,
    display: Display,
    location: PlacementLocation,
) -> Response<'static> {
    if display.image_id == 0 && display.image_number == 0 {
        return error_response(b"EINVAL: image ID or number required");
    }

    let mut response = Response {
        id: display.image_id,
        image_number: display.image_number,
        placement_id: display.placement_id,
        message: b"OK",
    };

    let image_id = if display.image_id != 0 {
        match storage.image_by_id(display.image_id) {
            Some(image) => image.id,
            None => {
                response.message = b"ENOENT: image not found";
                return response;
            }
        }
    } else {
        match storage.image_by_number(display.image_number) {
            Some(image) => image.id,
            None => {
                response.message = b"ENOENT: image not found";
                return response;
            }
        }
    };
    response.id = image_id;

    let location = if display.virtual_placement {
        if display.parent_id > 0 {
            response.message = b"EINVAL: virtual placement cannot refer to a parent";
            return response;
        }
        PlacementLocation::Virtual
    } else {
        location
    };

    let placement = Placement {
        location,
        x_offset: display.x_offset,
        y_offset: display.y_offset,
        source_x: display.x,
        source_y: display.y,
        source_width: display.width,
        source_height: display.height,
        columns: display.columns,
        rows: display.rows,
        z: display.z,
    };

    if let Err(err) = storage.add_placement(image_id, display.placement_id, placement) {
        encode_placement_error(&mut response, err);
    }

    response
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoadResult {
    id: u32,
    image_number: u32,
    placement_id: u32,
    implicit_id: bool,
    more: bool,
    display: Option<Display>,
}

fn load_and_add_image(
    storage: &mut ImageStorage,
    command: &Command,
    transmission: Transmission,
) -> Result<LoadResult, ImageLoadError> {
    if let Some(mut loading) = storage.loading.take() {
        loading.add_data(&command.data)?;

        if transmission.more_chunks {
            let result = load_result_from_loading(&loading, true);
            storage.loading = Some(loading);
            return Ok(result);
        }

        let display = loading.display;
        let mut image = loading.complete()?;
        let result = load_result_from_image(&image, display, false);
        let _ = storage.add_image(std::mem::take(&mut image))?;
        return Ok(result);
    }

    let mut loading = LoadingImage::init(command, storage.image_limits)?;
    assign_image_id(storage, &mut loading);

    if transmission.more_chunks {
        let result = load_result_from_loading(&loading, true);
        storage.loading = Some(Box::new(loading));
        return Ok(result);
    }

    let display = loading.display;
    let mut image = loading.complete()?;
    let result = load_result_from_image(&image, display, false);
    let _ = storage.add_image(std::mem::take(&mut image))?;
    Ok(result)
}

fn load_and_add_image_screen(
    screen: &mut Screen,
    command: &Command,
    transmission: Transmission,
) -> Result<LoadResult, ImageLoadError> {
    if let Some(mut loading) = screen.kitty_images_mut().loading.take() {
        loading.add_data(&command.data)?;

        if transmission.more_chunks {
            let result = load_result_from_loading(&loading, true);
            screen.kitty_images_mut().loading = Some(loading);
            return Ok(result);
        }

        let display = loading.display;
        let mut image = loading.complete()?;
        let result = load_result_from_image(&image, display, false);
        screen.add_kitty_image(std::mem::take(&mut image))?;
        return Ok(result);
    }

    let mut loading = LoadingImage::init(command, screen.kitty_images().image_limits)?;
    assign_image_id(screen.kitty_images_mut(), &mut loading);

    if transmission.more_chunks {
        let result = load_result_from_loading(&loading, true);
        screen.kitty_images_mut().loading = Some(Box::new(loading));
        return Ok(result);
    }

    let display = loading.display;
    let mut image = loading.complete()?;
    let result = load_result_from_image(&image, display, false);
    screen.add_kitty_image(std::mem::take(&mut image))?;
    Ok(result)
}

fn assign_image_id(storage: &mut ImageStorage, loading: &mut LoadingImage) {
    if loading.image.id != 0 {
        return;
    }

    loading.image.id = storage.next_image_id;
    storage.next_image_id = storage.next_image_id.wrapping_add(1);
    if loading.image.number == 0 {
        loading.image.implicit_id = true;
    }
}

fn load_result_from_loading(loading: &LoadingImage, more: bool) -> LoadResult {
    LoadResult {
        id: loading.image.id,
        image_number: loading.image.number,
        placement_id: 0,
        implicit_id: loading.image.implicit_id,
        more,
        display: loading.display,
    }
}

fn load_result_from_image(
    image: &super::graphics_image::Image,
    display: Option<Display>,
    more: bool,
) -> LoadResult {
    LoadResult {
        id: image.id,
        image_number: image.number,
        placement_id: 0,
        implicit_id: image.implicit_id,
        more,
        display,
    }
}

fn response_for_transmission(transmission: Transmission) -> Response<'static> {
    Response {
        id: transmission.image_id,
        image_number: transmission.image_number,
        placement_id: transmission.placement_id,
        message: b"OK",
    }
}

fn unimplemented_response_for_transmission(transmission: Transmission) -> Response<'static> {
    let mut response = response_for_transmission(transmission);
    response.message = b"ERROR: unimplemented action";
    response
}

fn unimplemented_response_for_display(display: Display) -> Response<'static> {
    Response {
        id: display.image_id,
        image_number: display.image_number,
        placement_id: display.placement_id,
        message: b"ERROR: unimplemented action",
    }
}

fn unimplemented_response_for_delete(delete: Delete) -> Response<'static> {
    let mut response = error_response(b"ERROR: unimplemented action");
    match delete {
        Delete::Id {
            image_id,
            placement_id,
            ..
        } => {
            response.id = image_id;
            response.placement_id = placement_id;
        }
        Delete::Newest {
            image_number,
            placement_id,
            ..
        } => {
            response.image_number = image_number;
            response.placement_id = placement_id;
        }
        Delete::All { .. }
        | Delete::IntersectCursor { .. }
        | Delete::AnimationFrames { .. }
        | Delete::IntersectCell { .. }
        | Delete::IntersectCellZ { .. }
        | Delete::Range { .. }
        | Delete::Column { .. }
        | Delete::Row { .. }
        | Delete::Z { .. } => {}
    }
    response
}

fn unimplemented_response_for_animation_frame(
    _animation: AnimationFrameLoading,
) -> Response<'static> {
    error_response(b"ERROR: unimplemented action")
}

fn unimplemented_response_for_animation_control(_animation: AnimationControl) -> Response<'static> {
    error_response(b"ERROR: unimplemented action")
}

fn unimplemented_response_for_animation_composition(
    _composition: AnimationFrameComposition,
) -> Response<'static> {
    error_response(b"ERROR: unimplemented action")
}

fn error_response(message: &'static [u8]) -> Response<'static> {
    Response {
        message,
        ..Response::default()
    }
}

fn encode_error(response: &mut Response<'static>, error: ImageLoadError) {
    response.message = match error {
        ImageLoadError::InvalidData => b"EINVAL: invalid data",
        ImageLoadError::DecompressionFailed => b"EINVAL: decompression failed",
        ImageLoadError::DimensionsRequired => b"EINVAL: dimensions required",
        ImageLoadError::DimensionsTooLarge => b"EINVAL: dimensions too large",
        ImageLoadError::TemporaryFileNotInTempDir
        | ImageLoadError::TemporaryFileNotNamedCorrectly => b"EINVAL: invalid data",
        ImageLoadError::UnsupportedFormat => b"EINVAL: unsupported format",
        ImageLoadError::UnsupportedMedium => b"EINVAL: unsupported medium",
        ImageLoadError::OutOfMemory => b"ENOMEM: out of memory",
    };
}

fn encode_placement_error(response: &mut Response<'static>, error: PlacementError) {
    response.message = match error {
        PlacementError::ImageNotFound => b"EINVAL: failed to prepare terminal state",
    };
}

fn filter_response(response: Option<Response<'static>>, quiet: Quiet) -> Option<Response<'static>> {
    let response = response?;
    match quiet {
        Quiet::No => (!response.empty() || !response.ok()).then_some(response),
        Quiet::Ok => (!response.ok()).then_some(response),
        Quiet::Failures => None,
    }
}

#[cfg(test)]
mod tests {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    use super::super::graphics_command::{
        AnimationControl, AnimationFrameComposition, AnimationFrameLoading, Command,
        CommandControl, CursorMovement, Delete, Display, Quiet, Transmission,
        TransmissionCompression, TransmissionFormat, TransmissionMedium,
    };
    use super::super::graphics_image::LoadingImageLimits;
    use super::super::graphics_storage::{
        ImageStorage, PlacementId, PlacementKey, PlacementLocation, DEFAULT_NEXT_IMAGE_ID,
    };
    use super::*;

    fn transmit(transmission: Transmission, data: &[u8]) -> Command {
        Command {
            control: CommandControl::Transmit(transmission),
            quiet: Quiet::No,
            data: data.to_vec(),
        }
    }

    fn query(transmission: Transmission, data: &[u8]) -> Command {
        Command {
            control: CommandControl::Query(transmission),
            quiet: Quiet::No,
            data: data.to_vec(),
        }
    }

    fn rgba_transmission(id: u32) -> Transmission {
        Transmission {
            image_id: id,
            width: 1,
            height: 2,
            ..Transmission::default()
        }
    }

    fn rgba_data() -> Vec<u8> {
        vec![0xff; 8]
    }

    fn display(image_id: u32) -> Display {
        Display {
            image_id,
            cursor_movement: CursorMovement::None,
            ..Display::default()
        }
    }

    fn location() -> PlacementLocation {
        PlacementLocation::Cell { x: 11, y: 13 }
    }

    fn store_rgba(storage: &mut ImageStorage, image_id: u32) {
        let command = transmit(
            Transmission {
                image_id,
                width: 1,
                height: 2,
                ..Transmission::default()
            },
            &rgba_data(),
        );
        assert!(execute(storage, &command).unwrap().ok());
    }

    fn store_numbered_rgba(storage: &mut ImageStorage, image_number: u32) -> u32 {
        let expected_id = storage.next_image_id;
        let command = transmit(
            Transmission {
                image_number,
                width: 1,
                height: 2,
                ..Transmission::default()
            },
            &rgba_data(),
        );
        let response = execute(storage, &command).unwrap();
        assert!(response.ok());
        assert_eq!(response.id, expected_id);
        expected_id
    }

    fn response_message(response: Option<Response<'static>>) -> &'static [u8] {
        response.unwrap().message
    }

    #[test]
    fn kitty_graphics_exec_query_success_does_not_store_image() {
        let mut storage = ImageStorage::new();
        let command = query(rgba_transmission(7), &rgba_data());

        let response = execute(&mut storage, &command).unwrap();

        assert!(response.ok());
        assert_eq!(response.id, 7);
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn kitty_graphics_exec_query_without_image_id_is_silent() {
        let mut storage = ImageStorage::new();
        let command = query(rgba_transmission(0), &rgba_data());

        assert!(execute(&mut storage, &command).is_none());
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn kitty_graphics_exec_transmit_stores_image_by_id() {
        let mut storage = ImageStorage::new();
        let command = transmit(rgba_transmission(3), &rgba_data());

        let response = execute(&mut storage, &command).unwrap();

        assert!(response.ok());
        assert_eq!(response.id, 3);
        let image = storage.image_by_id(3).unwrap();
        assert_eq!(image.data, rgba_data());
        assert_eq!(storage.total_bytes, 8);
    }

    #[test]
    fn kitty_graphics_exec_transmit_rejects_id_and_number_together() {
        let mut storage = ImageStorage::new();
        let command = transmit(
            Transmission {
                image_id: 3,
                image_number: 4,
                width: 1,
                height: 2,
                ..Transmission::default()
            },
            &rgba_data(),
        );

        let response = execute(&mut storage, &command).unwrap();

        assert_eq!(
            response.message,
            b"EINVAL: image ID and number are mutually exclusive"
        );
        assert_eq!(response.id, 3);
        assert_eq!(response.image_number, 4);
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn kitty_graphics_exec_implicit_id_assignment_suppresses_response() {
        let mut storage = ImageStorage::new();
        let command = transmit(rgba_transmission(0), &rgba_data());

        assert!(execute(&mut storage, &command).is_none());

        let image = storage.image_by_id(DEFAULT_NEXT_IMAGE_ID).unwrap();
        assert!(image.implicit_id);
        assert_eq!(storage.next_image_id, DEFAULT_NEXT_IMAGE_ID.wrapping_add(1));
    }

    #[test]
    fn kitty_graphics_exec_explicit_number_gets_auto_id_and_response() {
        let mut storage = ImageStorage::new();
        let command = transmit(
            Transmission {
                image_number: 9,
                width: 1,
                height: 2,
                ..Transmission::default()
            },
            &rgba_data(),
        );

        let response = execute(&mut storage, &command).unwrap();

        assert!(response.ok());
        assert_eq!(response.id, DEFAULT_NEXT_IMAGE_ID);
        assert_eq!(response.image_number, 9);
        assert!(
            !storage
                .image_by_id(DEFAULT_NEXT_IMAGE_ID)
                .unwrap()
                .implicit_id
        );
    }

    #[test]
    fn kitty_graphics_exec_storage_disabled_suppresses_responses_and_mutations() {
        let mut storage = ImageStorage::new();
        storage.set_limit(0);
        let command = transmit(rgba_transmission(3), &rgba_data());

        assert!(execute(&mut storage, &command).is_none());
        assert_eq!(storage.len(), 0);
        assert!(storage.image_by_id(3).is_none());
    }

    #[test]
    fn kitty_graphics_exec_more_chunks_with_q1_suppresses_final_ok() {
        let mut storage = ImageStorage::new();
        let mut first = transmit(
            Transmission {
                image_id: 1,
                width: 1,
                height: 2,
                more_chunks: true,
                ..Transmission::default()
            },
            &[0xff; 4],
        );
        first.quiet = Quiet::Ok;
        assert!(execute(&mut storage, &first).is_none());
        assert!(storage.loading.is_some());

        let second = transmit(
            Transmission {
                more_chunks: false,
                ..Transmission::default()
            },
            &[0xff; 4],
        );
        assert!(execute(&mut storage, &second).is_none());

        assert!(storage.loading.is_none());
        assert!(storage.image_by_id(1).is_some());
    }

    #[test]
    fn kitty_graphics_exec_more_chunks_with_q0_returns_final_ok() {
        let mut storage = ImageStorage::new();
        let first = transmit(
            Transmission {
                image_id: 1,
                width: 1,
                height: 2,
                more_chunks: true,
                ..Transmission::default()
            },
            &[0xff; 4],
        );
        assert!(execute(&mut storage, &first).is_none());

        let second = transmit(
            Transmission {
                more_chunks: false,
                ..Transmission::default()
            },
            &[0xff; 4],
        );
        let response = execute(&mut storage, &second).unwrap();

        assert!(response.ok());
        assert_eq!(response.id, 1);
        assert!(storage.loading.is_none());
        assert!(storage.image_by_id(1).is_some());
    }

    #[test]
    fn kitty_graphics_exec_more_chunks_can_increase_quiet() {
        let mut storage = ImageStorage::new();
        let first = transmit(
            Transmission {
                image_id: 1,
                width: 1,
                height: 2,
                more_chunks: true,
                ..Transmission::default()
            },
            &[0xff; 4],
        );
        assert!(execute(&mut storage, &first).is_none());

        let mut second = transmit(
            Transmission {
                more_chunks: false,
                ..Transmission::default()
            },
            &[0xff; 4],
        );
        second.quiet = Quiet::Ok;
        assert!(execute(&mut storage, &second).is_none());

        assert!(storage.image_by_id(1).is_some());
    }

    #[test]
    fn kitty_graphics_exec_default_format_is_rgba_after_transmit() {
        let mut storage = ImageStorage::new();
        let command = transmit(rgba_transmission(1), &rgba_data());

        assert!(execute(&mut storage, &command).unwrap().ok());
        assert_eq!(
            storage.image_by_id(1).unwrap().format,
            TransmissionFormat::Rgba
        );
    }

    #[test]
    fn kitty_graphics_exec_invalid_data_response() {
        let mut storage = ImageStorage::new();
        let command = transmit(
            Transmission {
                image_id: 1,
                width: 2,
                height: 2,
                ..Transmission::default()
            },
            &rgba_data(),
        );

        assert_eq!(
            response_message(execute(&mut storage, &command)),
            b"EINVAL: invalid data"
        );
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn kitty_graphics_exec_unsupported_format_response() {
        let mut storage = ImageStorage::new();
        let command = transmit(
            Transmission {
                image_id: 1,
                width: 1,
                height: 2,
                format: TransmissionFormat::Png,
                ..Transmission::default()
            },
            b"not png",
        );

        assert_eq!(
            response_message(execute(&mut storage, &command)),
            b"EINVAL: unsupported format"
        );
    }

    #[test]
    fn kitty_graphics_exec_unsupported_medium_response() {
        let mut storage = ImageStorage::new();
        storage.image_limits = LoadingImageLimits::ALL;
        let command = transmit(
            Transmission {
                image_id: 1,
                width: 1,
                height: 2,
                medium: TransmissionMedium::SharedMemory,
                ..Transmission::default()
            },
            b"/tmp/image",
        );

        assert_eq!(
            response_message(execute(&mut storage, &command)),
            b"EINVAL: unsupported medium"
        );
    }

    #[test]
    fn kitty_graphics_exec_decompression_failed_response() {
        let mut storage = ImageStorage::new();
        let command = transmit(
            Transmission {
                image_id: 1,
                width: 1,
                height: 2,
                compression: TransmissionCompression::ZlibDeflate,
                ..Transmission::default()
            },
            b"not zlib",
        );

        assert_eq!(
            response_message(execute(&mut storage, &command)),
            b"EINVAL: decompression failed"
        );
    }

    #[test]
    fn kitty_graphics_exec_dimensions_required_response() {
        let mut storage = ImageStorage::new();
        let command = transmit(
            Transmission {
                image_id: 1,
                width: 0,
                height: 2,
                ..Transmission::default()
            },
            &rgba_data(),
        );

        assert_eq!(
            response_message(execute(&mut storage, &command)),
            b"EINVAL: dimensions required"
        );
    }

    #[test]
    fn kitty_graphics_exec_storage_out_of_memory_response() {
        let mut storage = ImageStorage::new();
        storage.set_limit(4);
        let command = transmit(rgba_transmission(1), &rgba_data());

        assert_eq!(
            response_message(execute(&mut storage, &command)),
            b"ENOMEM: out of memory"
        );
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn kitty_graphics_exec_final_chunk_failure_clears_loading() {
        let mut storage = ImageStorage::new();
        let first = transmit(
            Transmission {
                image_id: 1,
                width: 2,
                height: 2,
                more_chunks: true,
                ..Transmission::default()
            },
            &[0xff; 4],
        );
        assert!(execute(&mut storage, &first).is_none());
        assert!(storage.loading.is_some());

        let second = transmit(
            Transmission {
                more_chunks: false,
                ..Transmission::default()
            },
            &[0xff; 4],
        );
        assert_eq!(
            response_message(execute(&mut storage, &second)),
            b"EINVAL: invalid data"
        );
        assert!(storage.loading.is_none());
    }

    #[test]
    fn kitty_graphics_exec_transmit_and_display_is_unimplemented_without_store() {
        let mut storage = ImageStorage::new();
        let command = Command {
            control: CommandControl::TransmitAndDisplay {
                transmission: rgba_transmission(1),
                display: Display::default(),
            },
            quiet: Quiet::No,
            data: rgba_data(),
        };

        let response = execute(&mut storage, &command).unwrap();

        assert_eq!(response.message, b"ERROR: unimplemented action");
        assert_eq!(response.id, 1);
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn kitty_graphics_exec_unimplemented_display_delete_animation_do_not_mutate() {
        let mut storage = ImageStorage::new();

        let display = Command {
            control: CommandControl::Display(Display {
                image_id: 1,
                cursor_movement: CursorMovement::None,
                ..Display::default()
            }),
            quiet: Quiet::No,
            data: Vec::new(),
        };
        assert_eq!(
            response_message(execute(&mut storage, &display)),
            b"ERROR: unimplemented action"
        );

        let delete = Command {
            control: CommandControl::Delete(Delete::Id {
                delete: true,
                image_id: 1,
                placement_id: 2,
            }),
            quiet: Quiet::No,
            data: Vec::new(),
        };
        assert_eq!(
            response_message(execute(&mut storage, &delete)),
            b"ERROR: unimplemented action"
        );

        for control in [
            CommandControl::TransmitAnimationFrame(AnimationFrameLoading::default()),
            CommandControl::ControlAnimation(AnimationControl::default()),
            CommandControl::ComposeAnimation(AnimationFrameComposition::default()),
        ] {
            let command = Command {
                control,
                quiet: Quiet::No,
                data: Vec::new(),
            };
            assert_eq!(
                response_message(execute(&mut storage, &command)),
                b"ERROR: unimplemented action"
            );
        }

        assert_eq!(storage.len(), 0);
        assert_eq!(storage.placement_len(), 0);
        assert!(storage.loading.is_none());
    }

    #[test]
    fn kitty_graphics_exec_unaddressed_delete_error_is_returned_with_default_quiet() {
        let mut storage = ImageStorage::new();
        let command = Command {
            control: CommandControl::Delete(Delete::All {
                delete_images: true,
            }),
            quiet: Quiet::No,
            data: Vec::new(),
        };

        let response = execute(&mut storage, &command).unwrap();

        assert_eq!(response.message, b"ERROR: unimplemented action");
        assert!(response.empty());
        assert_eq!(storage.len(), 0);
        assert!(storage.loading.is_none());
    }

    #[test]
    fn kitty_graphics_exec_query_maps_init_error() {
        let mut storage = ImageStorage::new();
        storage.image_limits = LoadingImageLimits::ALL;
        let command = query(
            Transmission {
                image_id: 1,
                medium: TransmissionMedium::SharedMemory,
                ..Transmission::default()
            },
            b"/tmp/image",
        );

        assert_eq!(
            response_message(execute(&mut storage, &command)),
            b"EINVAL: unsupported medium"
        );
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn kitty_graphics_exec_valid_zlib_stores_decompressed_data() {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&rgba_data()).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut storage = ImageStorage::new();
        let command = transmit(
            Transmission {
                image_id: 1,
                width: 1,
                height: 2,
                compression: TransmissionCompression::ZlibDeflate,
                ..Transmission::default()
            },
            &compressed,
        );

        assert!(execute(&mut storage, &command).unwrap().ok());
        assert_eq!(storage.image_by_id(1).unwrap().data, rgba_data());
    }

    #[test]
    fn kitty_graphics_exec_display_helper_requires_image_id_or_number() {
        let mut storage = ImageStorage::new();

        let response = display_with_location(&mut storage, Display::default(), location());

        assert_eq!(response.message, b"EINVAL: image ID or number required");
        assert!(response.empty());
        assert_eq!(storage.placement_len(), 0);
    }

    #[test]
    fn kitty_graphics_exec_display_helper_missing_image_errors_without_mutation() {
        let mut storage = ImageStorage::new();

        let response = display_with_location(&mut storage, display(99), location());

        assert_eq!(response.message, b"ENOENT: image not found");
        assert_eq!(response.id, 99);
        assert_eq!(storage.placement_len(), 0);
    }

    #[test]
    fn kitty_graphics_exec_display_helper_by_id_inserts_placement() {
        let mut storage = ImageStorage::new();
        store_rgba(&mut storage, 1);

        let response = display_with_location(&mut storage, display(1), location());

        assert!(response.ok());
        assert_eq!(response.id, 1);
        assert_eq!(storage.placement_len(), 1);
        let key = PlacementKey {
            image_id: 1,
            placement_id: PlacementId::Internal(0),
        };
        assert_eq!(storage.placement_by_key(key).unwrap().location, location());
    }

    #[test]
    fn kitty_graphics_exec_display_helper_by_number_resolves_newest_image_id() {
        let mut storage = ImageStorage::new();
        let _first_id = store_numbered_rgba(&mut storage, 7);
        let second_id = store_numbered_rgba(&mut storage, 7);
        let display = Display {
            image_number: 7,
            cursor_movement: CursorMovement::None,
            ..Display::default()
        };

        let response = display_with_location(&mut storage, display, location());

        assert!(response.ok());
        assert_eq!(response.id, second_id);
        assert_eq!(response.image_number, 7);
        assert_eq!(storage.placements_for_image(second_id).len(), 1);
    }

    #[test]
    fn kitty_graphics_exec_display_helper_maps_display_fields_to_placement() {
        let mut storage = ImageStorage::new();
        store_rgba(&mut storage, 1);
        let display = Display {
            image_id: 1,
            placement_id: 5,
            x: 2,
            y: 3,
            width: 4,
            height: 5,
            x_offset: 6,
            y_offset: 7,
            columns: 8,
            rows: 9,
            cursor_movement: CursorMovement::After,
            z: -10,
            ..Display::default()
        };

        let response = display_with_location(&mut storage, display, location());

        assert!(response.ok());
        assert_eq!(response.placement_id, 5);
        let key = PlacementKey {
            image_id: 1,
            placement_id: PlacementId::External(5),
        };
        let placement = storage.placement_by_key(key).unwrap();
        assert_eq!(placement.location, location());
        assert_eq!(placement.source_x, 2);
        assert_eq!(placement.source_y, 3);
        assert_eq!(placement.source_width, 4);
        assert_eq!(placement.source_height, 5);
        assert_eq!(placement.x_offset, 6);
        assert_eq!(placement.y_offset, 7);
        assert_eq!(placement.columns, 8);
        assert_eq!(placement.rows, 9);
        assert_eq!(placement.z, -10);
    }

    #[test]
    fn kitty_graphics_exec_display_helper_zero_placement_id_keeps_zero_response() {
        let mut storage = ImageStorage::new();
        store_rgba(&mut storage, 1);

        let response = display_with_location(&mut storage, display(1), location());

        assert!(response.ok());
        assert_eq!(response.placement_id, 0);
        assert!(storage
            .placement_by_key(PlacementKey {
                image_id: 1,
                placement_id: PlacementId::Internal(0),
            })
            .is_some());
    }

    #[test]
    fn kitty_graphics_exec_display_helper_external_placement_replaces() {
        let mut storage = ImageStorage::new();
        store_rgba(&mut storage, 1);
        let display = Display {
            image_id: 1,
            placement_id: 5,
            cursor_movement: CursorMovement::None,
            ..Display::default()
        };

        assert!(display_with_location(&mut storage, display, location()).ok());
        assert!(display_with_location(
            &mut storage,
            display,
            PlacementLocation::Cell { x: 17, y: 19 },
        )
        .ok());

        assert_eq!(storage.placement_len(), 1);
        assert_eq!(
            storage
                .placement_by_key(PlacementKey {
                    image_id: 1,
                    placement_id: PlacementId::External(5),
                })
                .unwrap()
                .location,
            PlacementLocation::Cell { x: 17, y: 19 }
        );
    }

    #[test]
    fn kitty_graphics_exec_display_helper_virtual_placement_overrides_location() {
        let mut storage = ImageStorage::new();
        store_rgba(&mut storage, 1);
        let display = Display {
            image_id: 1,
            virtual_placement: true,
            cursor_movement: CursorMovement::None,
            ..Display::default()
        };

        let response = display_with_location(&mut storage, display, location());

        assert!(response.ok());
        let key = PlacementKey {
            image_id: 1,
            placement_id: PlacementId::Internal(0),
        };
        assert_eq!(
            storage.placement_by_key(key).unwrap().location,
            PlacementLocation::Virtual
        );
    }

    #[test]
    fn kitty_graphics_exec_display_helper_virtual_parent_errors_without_mutation() {
        let mut storage = ImageStorage::new();
        store_rgba(&mut storage, 1);
        let display = Display {
            image_id: 1,
            virtual_placement: true,
            parent_id: 9,
            cursor_movement: CursorMovement::None,
            ..Display::default()
        };

        let response = display_with_location(&mut storage, display, location());

        assert_eq!(
            response.message,
            b"EINVAL: virtual placement cannot refer to a parent"
        );
        assert_eq!(storage.placement_len(), 0);
    }

    #[test]
    fn kitty_graphics_exec_display_helper_quiet_filtering() {
        let mut storage = ImageStorage::new();
        store_rgba(&mut storage, 1);

        let success = display_with_location(&mut storage, display(1), location());
        assert!(filter_response(Some(success), Quiet::Ok).is_none());

        let failure = display_with_location(&mut storage, display(99), location());
        assert_eq!(
            filter_response(Some(failure), Quiet::Ok).unwrap().message,
            b"ENOENT: image not found"
        );
        let hidden_failure = display_with_location(&mut storage, display(99), location());
        assert!(filter_response(Some(hidden_failure), Quiet::Failures).is_none());
    }
}

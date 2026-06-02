//! Kitty graphics image loading.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::time::Instant;

use flate2::read::ZlibDecoder;

use super::graphics_command::{
    Command, Display, Quiet, TransmissionCompression, TransmissionFormat, TransmissionMedium,
};

pub(crate) const MAX_DIMENSION: u32 = 10_000;
pub(crate) const MAX_IMAGE_SIZE: usize = 400 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageLoadError {
    InvalidData,
    DecompressionFailed,
    DimensionsRequired,
    DimensionsTooLarge,
    TemporaryFileNotInTempDir,
    TemporaryFileNotNamedCorrectly,
    UnsupportedFormat,
    UnsupportedMedium,
    OutOfMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LoadingImageLimits {
    pub(crate) file: bool,
    pub(crate) temporary_file: bool,
    pub(crate) shared_memory: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LoadingImage {
    pub(crate) image: Image,
    pub(crate) data: Vec<u8>,
    pub(crate) display: Option<Display>,
    pub(crate) quiet: Quiet,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub(crate) id: u32,
    pub(crate) number: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: TransmissionFormat,
    pub(crate) compression: TransmissionCompression,
    pub(crate) data: Vec<u8>,
    pub(crate) transmit_time: Option<Instant>,
    pub(crate) implicit_id: bool,
}

impl LoadingImageLimits {
    pub(crate) const ALL: Self = Self {
        file: true,
        temporary_file: true,
        shared_memory: true,
    };

    pub(crate) const DIRECT: Self = Self {
        file: false,
        temporary_file: false,
        shared_memory: false,
    };
}

impl Default for Image {
    fn default() -> Self {
        Self {
            id: 0,
            number: 0,
            width: 0,
            height: 0,
            format: TransmissionFormat::Rgb,
            compression: TransmissionCompression::None,
            data: Vec::new(),
            transmit_time: None,
            implicit_id: false,
        }
    }
}

impl Image {
    pub(crate) fn without_data(&self) -> Self {
        Self {
            id: self.id,
            number: self.number,
            width: self.width,
            height: self.height,
            format: self.format,
            compression: self.compression,
            data: Vec::new(),
            transmit_time: self.transmit_time,
            implicit_id: self.implicit_id,
        }
    }
}

impl LoadingImage {
    pub(crate) fn init(
        command: &Command,
        limits: LoadingImageLimits,
    ) -> Result<Self, ImageLoadError> {
        let transmission = command.transmission().ok_or(ImageLoadError::InvalidData)?;

        let mut result = Self {
            image: Image {
                id: transmission.image_id,
                number: transmission.image_number,
                width: transmission.width,
                height: transmission.height,
                compression: transmission.compression,
                format: transmission.format,
                implicit_id: transmission.image_id == 0 && transmission.image_number == 0,
                ..Image::default()
            },
            data: Vec::new(),
            display: command.display(),
            quiet: command.quiet,
        };

        match transmission.medium {
            TransmissionMedium::Direct => {
                result.add_data(&command.data)?;
                Ok(result)
            }
            TransmissionMedium::File => {
                if !limits.file || transmission.format == TransmissionFormat::Png {
                    return Err(ImageLoadError::UnsupportedMedium);
                }
                result.read_file(transmission, &command.data, false)?;
                Ok(result)
            }
            TransmissionMedium::TemporaryFile => {
                if !limits.temporary_file || transmission.format == TransmissionFormat::Png {
                    return Err(ImageLoadError::UnsupportedMedium);
                }
                result.read_file(transmission, &command.data, true)?;
                Ok(result)
            }
            TransmissionMedium::SharedMemory => {
                let _ = limits.shared_memory;
                Err(ImageLoadError::UnsupportedMedium)
            }
        }
    }

    pub(crate) fn add_data(&mut self, data: &[u8]) -> Result<(), ImageLoadError> {
        if data.is_empty() {
            return Ok(());
        }

        let new_len = self
            .data
            .len()
            .checked_add(data.len())
            .ok_or(ImageLoadError::InvalidData)?;
        if new_len > MAX_IMAGE_SIZE {
            return Err(ImageLoadError::InvalidData);
        }

        self.data
            .try_reserve(data.len())
            .map_err(|_| ImageLoadError::OutOfMemory)?;
        self.data.extend_from_slice(data);
        Ok(())
    }

    pub(crate) fn complete(mut self) -> Result<Image, ImageLoadError> {
        self.decompress()?;

        if self.image.format == TransmissionFormat::Png {
            return Err(ImageLoadError::UnsupportedFormat);
        }

        if self.image.width == 0 || self.image.height == 0 {
            return Err(ImageLoadError::DimensionsRequired);
        }
        if self.image.width > MAX_DIMENSION || self.image.height > MAX_DIMENSION {
            return Err(ImageLoadError::DimensionsTooLarge);
        }

        let bytes_per_pixel = self
            .image
            .format
            .bytes_per_pixel()
            .ok_or(ImageLoadError::UnsupportedFormat)?;
        let expected_len = (self.image.width as usize)
            .checked_mul(self.image.height as usize)
            .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
            .ok_or(ImageLoadError::InvalidData)?;
        if self.data.len() != expected_len {
            return Err(ImageLoadError::InvalidData);
        }

        self.image.transmit_time = Some(Instant::now());
        self.image.data = std::mem::take(&mut self.data);
        Ok(self.image)
    }

    fn decompress(&mut self) -> Result<(), ImageLoadError> {
        match self.image.compression {
            TransmissionCompression::None => Ok(()),
            TransmissionCompression::ZlibDeflate => self.decompress_zlib(),
        }
    }

    fn decompress_zlib(&mut self) -> Result<(), ImageLoadError> {
        let mut decoder = ZlibDecoder::new(self.data.as_slice());
        let mut output = Vec::new();
        let mut buf = [0u8; 8192];

        loop {
            let read = decoder
                .read(&mut buf)
                .map_err(|_| ImageLoadError::DecompressionFailed)?;
            if read == 0 {
                break;
            }

            let new_len = output
                .len()
                .checked_add(read)
                .ok_or(ImageLoadError::InvalidData)?;
            if new_len > MAX_IMAGE_SIZE {
                return Err(ImageLoadError::InvalidData);
            }

            output
                .try_reserve(read)
                .map_err(|_| ImageLoadError::OutOfMemory)?;
            output.extend_from_slice(&buf[..read]);
        }

        self.data = output;
        self.image.compression = TransmissionCompression::None;
        Ok(())
    }

    fn read_file(
        &mut self,
        transmission: super::graphics_command::Transmission,
        path_data: &[u8],
        temporary: bool,
    ) -> Result<(), ImageLoadError> {
        if path_data.contains(&0) {
            return Err(ImageLoadError::InvalidData);
        }

        let path = PathBuf::from(OsString::from_vec(path_data.to_vec()));
        let path = fs::canonicalize(path).map_err(|_| ImageLoadError::InvalidData)?;
        if is_unsafe_path(&path) {
            return Err(ImageLoadError::InvalidData);
        }

        let _cleanup = if temporary {
            if !is_path_in_temp_dir(&path) {
                return Err(ImageLoadError::TemporaryFileNotInTempDir);
            }
            if !path
                .as_os_str()
                .as_bytes()
                .windows(b"tty-graphics-protocol".len())
                .any(|part| part == b"tty-graphics-protocol")
            {
                return Err(ImageLoadError::TemporaryFileNotNamedCorrectly);
            }
            Some(TemporaryFileCleanup { path: path.clone() })
        } else {
            None
        };

        let mut file = File::open(&path).map_err(|_| ImageLoadError::InvalidData)?;
        let metadata = file.metadata().map_err(|_| ImageLoadError::InvalidData)?;
        if !metadata.file_type().is_file() {
            return Err(ImageLoadError::InvalidData);
        }

        if transmission.offset > 0 {
            file.seek(SeekFrom::Start(u64::from(transmission.offset)))
                .map_err(|_| ImageLoadError::InvalidData)?;
        }

        let limit = if transmission.size > 0 {
            usize::try_from(transmission.size)
                .unwrap_or(MAX_IMAGE_SIZE)
                .min(MAX_IMAGE_SIZE)
        } else {
            MAX_IMAGE_SIZE
        };
        let mut data = Vec::new();
        file.take(limit as u64)
            .read_to_end(&mut data)
            .map_err(|_| ImageLoadError::InvalidData)?;
        self.data = data;
        Ok(())
    }
}

struct TemporaryFileCleanup {
    path: PathBuf,
}

impl Drop for TemporaryFileCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn is_unsafe_path(path: &Path) -> bool {
    let bytes = path.as_os_str().as_bytes();
    bytes.starts_with(b"/proc/")
        || bytes.starts_with(b"/sys/")
        || (bytes.starts_with(b"/dev/") && !bytes.starts_with(b"/dev/shm/"))
}

fn is_path_in_temp_dir(path: &Path) -> bool {
    let path_bytes = path.as_os_str().as_bytes();
    if path_bytes.starts_with(b"/tmp") || path_bytes.starts_with(b"/dev/shm") {
        return true;
    }

    let temp = std::env::temp_dir();
    if path.starts_with(&temp) {
        return true;
    }

    if let Ok(temp) = fs::canonicalize(temp) {
        if path.starts_with(temp) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::super::graphics_command::{
        CommandControl, Transmission, TransmissionCompression, TransmissionFormat,
        TransmissionMedium,
    };
    use super::*;
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    const RGB_NONE_20X15: &[u8] = include_bytes!(
        "../../../../vendor/ghostty/src/terminal/kitty/testdata/image-rgb-none-20x15-2147483647-raw.data"
    );
    const RGB_ZLIB_128X96: &[u8] = include_bytes!(
        "../../../../vendor/ghostty/src/terminal/kitty/testdata/image-rgb-zlib_deflate-128x96-2147483647-raw.data"
    );

    fn transmit_command(transmission: Transmission, data: &[u8]) -> Command {
        Command {
            control: CommandControl::Transmit(transmission),
            quiet: Quiet::No,
            data: data.to_vec(),
        }
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn temp() -> Self {
            Self::in_base(std::env::temp_dir())
        }

        fn target() -> Self {
            Self::in_base(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target"))
        }

        fn in_base(base: PathBuf) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = base.join(format!(
                "roastty-kitty-graphics-image-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn write(&self, name: &str, data: &[u8]) -> PathBuf {
            let path = self.path.join(name);
            fs::write(&path, data).unwrap();
            fs::canonicalize(path).unwrap()
        }

        fn mkdir(&self, name: &str) -> PathBuf {
            let path = self.path.join(name);
            fs::create_dir(&path).unwrap();
            fs::canonicalize(path).unwrap()
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn path_bytes(path: &Path) -> Vec<u8> {
        path.as_os_str().as_bytes().to_vec()
    }

    #[test]
    fn kitty_graphics_image_load_with_invalid_rgb_data_allowed_at_init() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                width: 1,
                height: 1,
                image_id: 31,
                ..Transmission::default()
            },
            b"AAAA",
        );

        let loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        assert_eq!(loading.image.id, 31);
        assert_eq!(loading.data, b"AAAA");
    }

    #[test]
    fn kitty_graphics_image_too_wide_errors_on_complete() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                width: MAX_DIMENSION + 1,
                height: 1,
                image_id: 31,
                ..Transmission::default()
            },
            b"AAAA",
        );

        let loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        assert_eq!(loading.complete(), Err(ImageLoadError::DimensionsTooLarge));
    }

    #[test]
    fn kitty_graphics_image_too_tall_errors_on_complete() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                width: 1,
                height: MAX_DIMENSION + 1,
                image_id: 31,
                ..Transmission::default()
            },
            b"AAAA",
        );

        let loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        assert_eq!(loading.complete(), Err(ImageLoadError::DimensionsTooLarge));
    }

    #[test]
    fn kitty_graphics_image_rgb_zlib_compressed_direct() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::Direct,
                compression: TransmissionCompression::ZlibDeflate,
                width: 128,
                height: 96,
                image_id: 31,
                ..Transmission::default()
            },
            RGB_ZLIB_128X96,
        );

        let loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(image.compression, TransmissionCompression::None);
        assert_eq!(image.data.len(), 128 * 96 * 3);
    }

    #[test]
    fn kitty_graphics_image_rgb_uncompressed_direct() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::Direct,
                compression: TransmissionCompression::None,
                width: 20,
                height: 15,
                image_id: 31,
                ..Transmission::default()
            },
            RGB_NONE_20X15,
        );

        let loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(image.compression, TransmissionCompression::None);
        assert_eq!(image.data.len(), 20 * 15 * 3);
    }

    #[test]
    fn kitty_graphics_image_rgb_zlib_compressed_direct_chunked() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::Direct,
                compression: TransmissionCompression::ZlibDeflate,
                width: 128,
                height: 96,
                image_id: 31,
                more_chunks: true,
                ..Transmission::default()
            },
            &RGB_ZLIB_128X96[..1024],
        );

        let mut loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        for chunk in RGB_ZLIB_128X96[1024..].chunks(1024) {
            loading.add_data(chunk).unwrap();
        }

        let image = loading.complete().unwrap();
        assert_eq!(image.compression, TransmissionCompression::None);
        assert_eq!(image.data.len(), 128 * 96 * 3);
    }

    #[test]
    fn kitty_graphics_image_rgb_zlib_compressed_direct_chunked_zero_initial_chunk() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::Direct,
                compression: TransmissionCompression::ZlibDeflate,
                width: 128,
                height: 96,
                image_id: 31,
                more_chunks: true,
                ..Transmission::default()
            },
            b"",
        );

        let mut loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        for chunk in RGB_ZLIB_128X96.chunks(1024) {
            loading.add_data(chunk).unwrap();
        }

        let image = loading.complete().unwrap();
        assert_eq!(image.compression, TransmissionCompression::None);
        assert_eq!(image.data.len(), 128 * 96 * 3);
    }

    #[test]
    fn kitty_graphics_image_direct_medium_always_allowed_by_limits() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::Direct,
                width: 1,
                height: 1,
                image_id: 31,
                ..Transmission::default()
            },
            b"AAAA",
        );

        let loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        assert_eq!(loading.image.id, 31);
    }

    #[test]
    fn kitty_graphics_image_final_byte_length_mismatch_errors() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::Direct,
                width: 2,
                height: 2,
                image_id: 31,
                ..Transmission::default()
            },
            b"AAAA",
        );

        let loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        assert_eq!(loading.complete(), Err(ImageLoadError::InvalidData));
    }

    #[test]
    fn kitty_graphics_image_missing_dimensions_error() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::Direct,
                image_id: 31,
                ..Transmission::default()
            },
            b"AAAA",
        );

        let loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        assert_eq!(loading.complete(), Err(ImageLoadError::DimensionsRequired));
    }

    #[test]
    fn kitty_graphics_image_malformed_zlib_errors() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::Direct,
                compression: TransmissionCompression::ZlibDeflate,
                width: 1,
                height: 1,
                image_id: 31,
                ..Transmission::default()
            },
            b"not zlib",
        );

        let loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        assert_eq!(loading.complete(), Err(ImageLoadError::DecompressionFailed));
    }

    #[test]
    fn kitty_graphics_image_png_direct_deferred() {
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Png,
                medium: TransmissionMedium::Direct,
                width: 1,
                height: 1,
                image_id: 31,
                ..Transmission::default()
            },
            b"not a png",
        );

        let loading = LoadingImage::init(&command, LoadingImageLimits::DIRECT).unwrap();
        assert_eq!(loading.complete(), Err(ImageLoadError::UnsupportedFormat));
    }

    #[test]
    fn kitty_graphics_image_file_medium_blocked_and_allowed_by_limits() {
        let dir = TestDir::temp();
        let path = dir.write("image.data", RGB_NONE_20X15);
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::File,
                width: 20,
                height: 15,
                image_id: 31,
                ..Transmission::default()
            },
            &path_bytes(&path),
        );

        assert_eq!(
            LoadingImage::init(&command, LoadingImageLimits::DIRECT),
            Err(ImageLoadError::UnsupportedMedium)
        );

        let loading = LoadingImage::init(
            &command,
            LoadingImageLimits {
                file: true,
                temporary_file: false,
                shared_memory: false,
            },
        )
        .unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(image.data, RGB_NONE_20X15);
        assert!(path.exists());
    }

    #[test]
    fn kitty_graphics_image_temporary_file_medium_blocked_and_allowed_by_limits() {
        let dir = TestDir::temp();
        let path = dir.write("tty-graphics-protocol-image.data", RGB_NONE_20X15);
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::TemporaryFile,
                width: 20,
                height: 15,
                image_id: 31,
                ..Transmission::default()
            },
            &path_bytes(&path),
        );

        assert_eq!(
            LoadingImage::init(
                &command,
                LoadingImageLimits {
                    file: true,
                    temporary_file: false,
                    shared_memory: true,
                },
            ),
            Err(ImageLoadError::UnsupportedMedium)
        );
        assert!(path.exists());

        let loading = LoadingImage::init(
            &command,
            LoadingImageLimits {
                file: false,
                temporary_file: true,
                shared_memory: false,
            },
        )
        .unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(image.data, RGB_NONE_20X15);
        assert!(!path.exists());
    }

    #[test]
    fn kitty_graphics_image_temporary_file_validation_controls_cleanup() {
        let dir = TestDir::temp();
        let wrong_name = dir.write("image.data", RGB_NONE_20X15);
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::TemporaryFile,
                width: 20,
                height: 15,
                image_id: 31,
                ..Transmission::default()
            },
            &path_bytes(&wrong_name),
        );
        assert_eq!(
            LoadingImage::init(&command, LoadingImageLimits::ALL),
            Err(ImageLoadError::TemporaryFileNotNamedCorrectly)
        );
        assert!(wrong_name.exists());

        let outside = TestDir::target();
        let outside_path = outside.write("tty-graphics-protocol-image.data", RGB_NONE_20X15);
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::TemporaryFile,
                width: 20,
                height: 15,
                image_id: 32,
                ..Transmission::default()
            },
            &path_bytes(&outside_path),
        );
        assert_eq!(
            LoadingImage::init(&command, LoadingImageLimits::ALL),
            Err(ImageLoadError::TemporaryFileNotInTempDir)
        );
        assert!(outside_path.exists());

        let invalid = dir.write("tty-graphics-protocol-invalid.data", b"AAAA");
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::TemporaryFile,
                width: 20,
                height: 15,
                image_id: 33,
                ..Transmission::default()
            },
            &path_bytes(&invalid),
        );
        let loading = LoadingImage::init(&command, LoadingImageLimits::ALL).unwrap();
        assert_eq!(loading.complete(), Err(ImageLoadError::InvalidData));
        assert!(!invalid.exists());
    }

    #[test]
    fn kitty_graphics_image_file_media_offset_size_and_invalid_paths() {
        let dir = TestDir::temp();
        let mut padded = b"XX".to_vec();
        padded.extend_from_slice(RGB_NONE_20X15);
        padded.extend_from_slice(b"YY");
        let path = dir.write("image.data", &padded);
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::File,
                width: 20,
                height: 15,
                image_id: 31,
                offset: 2,
                size: u32::try_from(RGB_NONE_20X15.len()).unwrap(),
                ..Transmission::default()
            },
            &path_bytes(&path),
        );
        let image = LoadingImage::init(&command, LoadingImageLimits::ALL)
            .unwrap()
            .complete()
            .unwrap();
        assert_eq!(image.data, RGB_NONE_20X15);

        let directory = dir.mkdir("directory.data");
        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::File,
                width: 1,
                height: 1,
                image_id: 32,
                ..Transmission::default()
            },
            &path_bytes(&directory),
        );
        assert_eq!(
            LoadingImage::init(&command, LoadingImageLimits::ALL),
            Err(ImageLoadError::InvalidData)
        );

        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::File,
                width: 1,
                height: 1,
                image_id: 33,
                ..Transmission::default()
            },
            b"/tmp/roastty\0image",
        );
        assert_eq!(
            LoadingImage::init(&command, LoadingImageLimits::ALL),
            Err(ImageLoadError::InvalidData)
        );

        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::File,
                width: 1,
                height: 1,
                image_id: 34,
                ..Transmission::default()
            },
            b"/dev/null",
        );
        assert_eq!(
            LoadingImage::init(&command, LoadingImageLimits::ALL),
            Err(ImageLoadError::InvalidData)
        );
    }

    #[test]
    fn kitty_graphics_image_non_direct_png_and_shared_memory_remain_deferred() {
        let dir = TestDir::temp();
        let png_temp = dir.write("tty-graphics-protocol-image.png", b"not a png");

        for medium in [TransmissionMedium::File, TransmissionMedium::TemporaryFile] {
            let command = transmit_command(
                Transmission {
                    format: TransmissionFormat::Png,
                    medium,
                    width: 0,
                    height: 0,
                    image_id: 31,
                    ..Transmission::default()
                },
                &path_bytes(&png_temp),
            );
            assert_eq!(
                LoadingImage::init(&command, LoadingImageLimits::ALL),
                Err(ImageLoadError::UnsupportedMedium)
            );
        }
        assert!(png_temp.exists());

        let command = transmit_command(
            Transmission {
                format: TransmissionFormat::Rgb,
                medium: TransmissionMedium::SharedMemory,
                width: 1,
                height: 1,
                image_id: 31,
                ..Transmission::default()
            },
            b"/shared",
        );
        assert_eq!(
            LoadingImage::init(&command, LoadingImageLimits::ALL),
            Err(ImageLoadError::UnsupportedMedium)
        );
    }

    #[test]
    fn kitty_graphics_image_without_data_preserves_metadata_only() {
        let image = Image {
            id: 31,
            number: 7,
            width: 20,
            height: 15,
            format: TransmissionFormat::Rgb,
            compression: TransmissionCompression::None,
            data: RGB_NONE_20X15.to_vec(),
            transmit_time: Some(Instant::now()),
            implicit_id: true,
        };

        let without_data = image.without_data();
        assert_eq!(without_data.id, image.id);
        assert_eq!(without_data.number, image.number);
        assert_eq!(without_data.width, image.width);
        assert_eq!(without_data.height, image.height);
        assert_eq!(without_data.format, image.format);
        assert_eq!(without_data.compression, image.compression);
        assert_eq!(without_data.transmit_time, image.transmit_time);
        assert_eq!(without_data.implicit_id, image.implicit_id);
        assert!(without_data.data.is_empty());
        assert_eq!(image.data, RGB_NONE_20X15);
    }
}

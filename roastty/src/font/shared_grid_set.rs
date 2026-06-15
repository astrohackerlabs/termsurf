//! Shared ownership set for expensive [`SharedGrid`] instances.
//!
//! This is the ownership/refcount/locking foundation of upstream
//! `font/SharedGridSet.zig`. Roastty does not have the full upstream
//! config-derived font key yet, so the set is generic over the key and accepts a
//! caller-supplied grid constructor.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::config;
use crate::font::codepoint_map::CodepointMap;
use crate::font::codepoint_resolver::CodepointResolver;
use crate::font::collection::{Collection, CompleteError, SetPointSizeError, SyntheticStyle};
use crate::font::discovery::Descriptor;
use crate::font::face::coretext::Face;
use crate::font::shared_grid::SharedGrid;
use crate::font::Style;

/// A shared grid reference returned by [`SharedGridSet::ref_grid`].
pub(crate) struct SharedGridHandle<K> {
    key: K,
    grid: Arc<Mutex<SharedGrid>>,
}

impl<K> SharedGridHandle<K> {
    pub(crate) fn key(&self) -> &K {
        &self.key
    }

    pub(crate) fn grid(&self) -> &Arc<Mutex<SharedGrid>> {
        &self.grid
    }
}

struct ReffedGrid {
    grid: Arc<Mutex<SharedGrid>>,
    refs: usize,
}

/// Result of releasing a grid reference from the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DerefResult {
    /// No grid existed for the key.
    Missing,
    /// The refcount was decremented and the grid remains cached.
    Decremented,
    /// The final reference was released and the grid was removed.
    Removed,
}

/// A keyed set of shared font grids with explicit set-owned refcounts.
pub(crate) struct SharedGridSet<K> {
    grids: Mutex<HashMap<K, ReffedGrid>>,
}

impl<K> Default for SharedGridSet<K> {
    fn default() -> Self {
        SharedGridSet {
            grids: Mutex::new(HashMap::new()),
        }
    }
}

impl<K> SharedGridSet<K>
where
    K: Clone + Eq + Hash,
{
    pub(crate) fn new() -> SharedGridSet<K> {
        SharedGridSet::default()
    }

    /// Returns the number of cached grids.
    pub(crate) fn count(&self) -> usize {
        self.grids
            .lock()
            .expect("shared grid set mutex poisoned")
            .len()
    }

    /// References the grid for `key`, constructing and caching one if needed.
    pub(crate) fn ref_grid<F>(&self, key: K, make_grid: F) -> SharedGridHandle<K>
    where
        F: FnOnce() -> SharedGrid,
    {
        let mut grids = self.grids.lock().expect("shared grid set mutex poisoned");

        if let Some(reffed) = grids.get_mut(&key) {
            reffed.refs = reffed
                .refs
                .checked_add(1)
                .expect("shared grid refcount overflow");
            return SharedGridHandle {
                key,
                grid: Arc::clone(&reffed.grid),
            };
        }

        let grid = Arc::new(Mutex::new(make_grid()));
        grids.insert(
            key.clone(),
            ReffedGrid {
                grid: Arc::clone(&grid),
                refs: 1,
            },
        );

        SharedGridHandle { key, grid }
    }

    /// Releases one reference for `key`.
    pub(crate) fn deref_grid(&self, key: &K) -> DerefResult {
        let mut grids = self.grids.lock().expect("shared grid set mutex poisoned");

        let Some(reffed) = grids.get_mut(key) else {
            return DerefResult::Missing;
        };

        if reffed.refs > 1 {
            reffed.refs -= 1;
            return DerefResult::Decremented;
        }

        grids.remove(key);
        DerefResult::Removed
    }

    #[cfg(test)]
    fn ref_count(&self, key: &K) -> Option<usize> {
        self.grids
            .lock()
            .expect("shared grid set mutex poisoned")
            .get(key)
            .map(|reffed| reffed.refs)
    }
}

/// Font config snapshot needed to build a shared grid key.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DerivedConfig {
    pub font_family: config::RepeatableString,
    pub font_family_bold: config::RepeatableString,
    pub font_family_italic: config::RepeatableString,
    pub font_family_bold_italic: config::RepeatableString,
    pub font_style: config::FontStyle,
    pub font_style_bold: config::FontStyle,
    pub font_style_italic: config::FontStyle,
    pub font_style_bold_italic: config::FontStyle,
    pub font_codepoint_map: config::RepeatableCodepointMap,
    pub font_synthetic_style: config::FontSyntheticStyle,
}

impl DerivedConfig {
    pub(crate) fn from_config(config: &config::Config) -> DerivedConfig {
        DerivedConfig {
            font_family: config.font_family.clone(),
            font_family_bold: config.font_family_bold.clone(),
            font_family_italic: config.font_family_italic.clone(),
            font_family_bold_italic: config.font_family_bold_italic.clone(),
            font_style: config.font_style.clone(),
            font_style_bold: config.font_style_bold.clone(),
            font_style_italic: config.font_style_italic.clone(),
            font_style_bold_italic: config.font_style_bold_italic.clone(),
            font_codepoint_map: config.font_codepoint_map.clone(),
            font_synthetic_style: config.font_synthetic_style,
        }
    }
}

/// The config-derived key for a shared font grid.
#[derive(Debug, Clone)]
pub(crate) struct Key {
    descriptors: Vec<Descriptor>,
    style_offsets: [usize; 4],
    codepoint_map: CodepointMap,
    font_size_points: f32,
}

impl Key {
    pub(crate) fn new(config: &DerivedConfig, font_size_points: f32) -> Key {
        let mut descriptors = Vec::new();

        append_descriptors(
            &mut descriptors,
            &config.font_family,
            &config.font_style,
            Style::Regular,
            font_size_points,
        );
        let regular_offset = descriptors.len();
        append_descriptors(
            &mut descriptors,
            &config.font_family_bold,
            &config.font_style_bold,
            Style::Bold,
            font_size_points,
        );
        let bold_offset = descriptors.len();
        append_descriptors(
            &mut descriptors,
            &config.font_family_italic,
            &config.font_style_italic,
            Style::Italic,
            font_size_points,
        );
        let italic_offset = descriptors.len();
        append_descriptors(
            &mut descriptors,
            &config.font_family_bold_italic,
            &config.font_style_bold_italic,
            Style::BoldItalic,
            font_size_points,
        );
        let bold_italic_offset = descriptors.len();

        Key {
            descriptors,
            style_offsets: [
                regular_offset,
                bold_offset,
                italic_offset,
                bold_italic_offset,
            ],
            codepoint_map: config.font_codepoint_map.map.clone(),
            font_size_points,
        }
    }

    pub(crate) fn descriptors_for_style(&self, style: Style) -> &[Descriptor] {
        let idx = style as usize;
        let start = if idx == 0 {
            0
        } else {
            self.style_offsets[idx - 1]
        };
        let end = self.style_offsets[idx];
        &self.descriptors[start..end]
    }

    pub(crate) fn codepoint_map(&self) -> &CodepointMap {
        &self.codepoint_map
    }

    pub(crate) fn hashcode(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut h);
        h.finish()
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.font_size_points.to_bits() == other.font_size_points.to_bits()
            && self.style_offsets == other.style_offsets
            && self.descriptors == other.descriptors
            && self.codepoint_map == other.codepoint_map
    }
}

impl Eq for Key {}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_size_points.to_bits().hash(state);
        self.style_offsets.hash(state);
        self.descriptors.len().hash(state);
        for descriptor in &self.descriptors {
            descriptor.hashcode().hash(state);
        }
        self.codepoint_map.hashcode().hash(state);
    }
}

fn append_descriptors(
    descriptors: &mut Vec<Descriptor>,
    families: &config::RepeatableString,
    style_config: &config::FontStyle,
    style: Style,
    font_size_points: f32,
) {
    let exact_style = style_config.name_value();
    for family in &families.list {
        let mut descriptor = Descriptor {
            family: Some(family.clone()),
            style: exact_style.map(ToOwned::to_owned),
            size: font_size_points,
            monospace: true,
            ..Default::default()
        };
        if exact_style.is_none() {
            match style {
                Style::Regular => {}
                Style::Bold => descriptor.bold = true,
                Style::Italic => descriptor.italic = true,
                Style::BoldItalic => {
                    descriptor.bold = true;
                    descriptor.italic = true;
                }
            }
        }
        descriptors.push(descriptor);
    }
}

/// Errors building a shared grid from config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuildGridError {
    MissingPrimaryFont,
    CompleteStyles,
    InvalidPointSize,
}

impl From<CompleteError> for BuildGridError {
    fn from(_: CompleteError) -> Self {
        BuildGridError::CompleteStyles
    }
}

impl From<SetPointSizeError> for BuildGridError {
    fn from(error: SetPointSizeError) -> Self {
        match error {
            SetPointSizeError::InvalidPointSize => BuildGridError::InvalidPointSize,
            SetPointSizeError::CannotLoadPrimaryFont => BuildGridError::MissingPrimaryFont,
        }
    }
}

/// Build a `SharedGrid` from represented config fields. This is the config
/// assembly half of upstream `SharedGridSet.ref`.
pub(crate) fn build_grid_from_config(
    config: &config::Config,
    font_size_points: f32,
) -> Result<SharedGrid, BuildGridError> {
    let mut config = config.clone();
    config.finalize();
    let derived = DerivedConfig::from_config(&config);
    let key = Key::new(&derived, font_size_points);
    build_grid_for_key(&key, &derived)
}

fn build_grid_for_key(key: &Key, config: &DerivedConfig) -> Result<SharedGrid, BuildGridError> {
    let collection = collection_for_key(key, config)?;
    let metrics = *collection
        .metrics()
        .ok_or(BuildGridError::MissingPrimaryFont)?;

    let mut resolver = CodepointResolver::new(collection);
    resolver.set_style_enabled(Style::Bold, config.font_style_bold.enabled());
    resolver.set_style_enabled(Style::Italic, config.font_style_italic.enabled());
    resolver.set_style_enabled(Style::BoldItalic, config.font_style_bold_italic.enabled());
    resolver.set_discover_enabled(true);
    if !key.codepoint_map().is_empty() {
        resolver.set_codepoint_map(Some(key.codepoint_map().clone()));
    }

    Ok(SharedGrid::new(resolver, metrics))
}

fn collection_for_key(key: &Key, config: &DerivedConfig) -> Result<Collection, BuildGridError> {
    let mut collection = Collection::new();

    for style in [
        Style::Regular,
        Style::Bold,
        Style::Italic,
        Style::BoldItalic,
    ] {
        for descriptor in key.descriptors_for_style(style) {
            if let Some(face) = descriptor.discover_deferred_faces().next() {
                collection
                    .add_deferred_with_adjustment(
                        face,
                        style,
                        false,
                        crate::font::collection::SizeAdjustment::None,
                    )
                    .map_err(|_| BuildGridError::MissingPrimaryFont)?;
            }
        }
    }

    if collection.face_count(Style::Regular) == 0 {
        collection
            .add(
                Face::new("Menlo", key.font_size_points.max(1.0) as f64),
                Style::Regular,
                false,
            )
            .map_err(|_| BuildGridError::MissingPrimaryFont)?;
    }

    collection.set_point_size(key.font_size_points.max(1.0) as f64)?;
    collection.complete_styles(SyntheticStyle::from(config.font_synthetic_style))?;
    add_apple_emoji_fallback(&mut collection)?;
    collection
        .update_metrics()
        .map_err(|_| BuildGridError::MissingPrimaryFont)?;
    Ok(collection)
}

fn add_apple_emoji_fallback(collection: &mut Collection) -> Result<(), BuildGridError> {
    let descriptor = Descriptor {
        family: Some("Apple Color Emoji".to_string()),
        ..Default::default()
    };
    if let Some(face) = descriptor.discover_deferred_faces().next() {
        collection
            .add_deferred_with_adjustment(
                face,
                Style::Regular,
                true,
                crate::font::collection::SizeAdjustment::None,
            )
            .map_err(|_| BuildGridError::MissingPrimaryFont)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::{Config, FontStyle, FontSyntheticStyle};
    use crate::font::codepoint_resolver::CodepointResolver;
    use crate::font::collection::{Collection, Index};
    use crate::font::face::coretext::Face;
    use crate::font::Style;

    fn menlo_grid() -> SharedGrid {
        let mut collection = Collection::new();
        collection
            .add(Face::new("Menlo", 32.0), Style::Regular, false)
            .unwrap();
        collection.update_metrics().unwrap();
        let metrics = *collection.metrics().unwrap();
        SharedGrid::new(CodepointResolver::new(collection), metrics)
    }

    #[test]
    fn ref_grid_reuses_cached_grid_and_counts_refs() {
        let set = SharedGridSet::new();

        let first = set.ref_grid("menlo", menlo_grid);
        assert_eq!(set.count(), 1);
        assert_eq!(set.ref_count(first.key()), Some(1));

        let second = set.ref_grid("menlo", menlo_grid);
        assert_eq!(set.count(), 1);
        assert_eq!(set.ref_count(first.key()), Some(2));
        assert!(Arc::ptr_eq(first.grid(), second.grid()));

        assert_eq!(set.deref_grid(second.key()), DerefResult::Decremented);
        assert_eq!(set.count(), 1);
        assert_eq!(set.ref_count(first.key()), Some(1));

        assert_eq!(set.deref_grid(first.key()), DerefResult::Removed);
        assert_eq!(set.count(), 0);
        assert_eq!(set.ref_count(first.key()), None);
    }

    #[test]
    fn different_keys_create_distinct_grids() {
        let set = SharedGridSet::new();

        let regular = set.ref_grid(("Menlo", 32), menlo_grid);
        let large = set.ref_grid(("Menlo", 40), menlo_grid);

        assert_eq!(set.count(), 2);
        assert_eq!(set.ref_count(regular.key()), Some(1));
        assert_eq!(set.ref_count(large.key()), Some(1));
        assert!(!Arc::ptr_eq(regular.grid(), large.grid()));
    }

    #[test]
    fn deref_missing_key_is_noop() {
        let set: SharedGridSet<&str> = SharedGridSet::new();

        assert_eq!(set.deref_grid(&"missing"), DerefResult::Missing);
        assert_eq!(set.count(), 0);
    }

    #[test]
    fn shared_handle_allows_mutable_grid_access() {
        let set = SharedGridSet::new();
        let handle = set.ref_grid("menlo", menlo_grid);

        let mut grid = handle.grid().lock().unwrap();
        let index = grid
            .get_index('A' as u32, Style::Regular, None)
            .unwrap()
            .expect("Menlo resolves A");

        assert!(grid.has_codepoint(index, 'A' as u32, None));
    }

    #[test]
    fn shared_grid_set_key_default_is_stable_and_size_sensitive() {
        let cfg = Config::default();
        let derived = DerivedConfig::from_config(&cfg);
        let key = Key::new(&derived, 13.0);
        let same = Key::new(&derived, 13.0);
        let larger = Key::new(&derived, 14.0);

        assert_eq!(key, same);
        assert_eq!(key.hashcode(), same.hashcode());
        assert_ne!(key, larger);
        assert_ne!(key.hashcode(), larger.hashcode());
        assert!(key.descriptors_for_style(Style::Regular).is_empty());
    }

    #[test]
    fn shared_grid_set_key_builds_style_ordered_descriptors() {
        let mut cfg = Config::default();
        cfg.font_family.parse_cli(Some("Regular")).unwrap();
        cfg.font_family_bold.parse_cli(Some("Bold")).unwrap();
        cfg.font_family_italic.parse_cli(Some("Italic")).unwrap();
        cfg.font_family_bold_italic
            .parse_cli(Some("Bold Italic"))
            .unwrap();
        cfg.font_style_bold = FontStyle::Name("Heavy".to_string());
        cfg.finalize();

        let derived = DerivedConfig::from_config(&cfg);
        let key = Key::new(&derived, 13.0);
        let regular = key.descriptors_for_style(Style::Regular);
        let bold = key.descriptors_for_style(Style::Bold);
        let italic = key.descriptors_for_style(Style::Italic);
        let bold_italic = key.descriptors_for_style(Style::BoldItalic);

        assert_eq!(regular.len(), 1);
        assert_eq!(regular[0].family.as_deref(), Some("Regular"));
        assert_eq!(regular[0].style, None);
        assert!(!regular[0].bold);
        assert!(!regular[0].italic);

        assert_eq!(bold.len(), 1);
        assert_eq!(bold[0].family.as_deref(), Some("Bold"));
        assert_eq!(bold[0].style.as_deref(), Some("Heavy"));
        assert!(!bold[0].bold, "exact style disables bold category search");

        assert_eq!(italic.len(), 1);
        assert_eq!(italic[0].family.as_deref(), Some("Italic"));
        assert!(italic[0].italic);

        assert_eq!(bold_italic.len(), 1);
        assert_eq!(bold_italic[0].family.as_deref(), Some("Bold Italic"));
        assert!(bold_italic[0].bold);
        assert!(bold_italic[0].italic);
    }

    #[test]
    fn shared_grid_set_key_preserves_multiple_family_order() {
        let mut cfg = Config::default();
        cfg.font_family.parse_cli(Some("Regular First")).unwrap();
        cfg.font_family.parse_cli(Some("Regular Second")).unwrap();
        cfg.font_family_bold.parse_cli(Some("Bold First")).unwrap();
        cfg.font_family_bold.parse_cli(Some("Bold Second")).unwrap();
        cfg.font_style = FontStyle::Name("Book".to_string());
        cfg.finalize();

        let derived = DerivedConfig::from_config(&cfg);
        let key = Key::new(&derived, 13.0);
        let regular = key.descriptors_for_style(Style::Regular);
        let bold = key.descriptors_for_style(Style::Bold);

        assert_eq!(regular.len(), 2);
        assert_eq!(regular[0].family.as_deref(), Some("Regular First"));
        assert_eq!(regular[1].family.as_deref(), Some("Regular Second"));
        assert_eq!(regular[0].style.as_deref(), Some("Book"));
        assert_eq!(regular[1].style.as_deref(), Some("Book"));
        assert!(!regular[0].bold);
        assert!(!regular[1].bold);

        assert_eq!(bold.len(), 2);
        assert_eq!(bold[0].family.as_deref(), Some("Bold First"));
        assert_eq!(bold[1].family.as_deref(), Some("Bold Second"));
        assert!(bold[0].bold);
        assert!(bold[1].bold);
    }

    #[test]
    fn shared_grid_set_key_includes_codepoint_map() {
        let mut cfg = Config::default();
        let base = Key::new(&DerivedConfig::from_config(&cfg), 13.0);
        cfg.font_codepoint_map
            .parse_cli(Some("U+0041-U+005A=Helvetica"))
            .unwrap();
        let mapped = Key::new(&DerivedConfig::from_config(&cfg), 13.0);

        assert_ne!(base, mapped);
        assert_ne!(base.hashcode(), mapped.hashcode());
        assert_eq!(mapped.codepoint_map().len(), 1);
    }

    #[test]
    fn shared_grid_set_build_grid_from_default_config() {
        let cfg = Config::default();
        let mut grid = build_grid_from_config(&cfg, 13.0).expect("grid builds");
        let index = grid
            .get_index('A' as u32, Style::Regular, None)
            .unwrap()
            .expect("default config resolves ASCII");
        assert!(grid.has_codepoint(index, 'A' as u32, None));
    }

    #[test]
    fn shared_grid_set_build_grid_honors_codepoint_override() {
        let mut cfg = Config::default();
        cfg.font_codepoint_map
            .parse_cli(Some("U+0041=Helvetica"))
            .unwrap();

        let mut grid = build_grid_from_config(&cfg, 13.0).expect("grid builds");
        let overridden = grid
            .get_index('A' as u32, Style::Regular, None)
            .unwrap()
            .expect("override resolves A");
        let regular = build_grid_from_config(&Config::default(), 13.0)
            .expect("plain grid builds")
            .get_index('A' as u32, Style::Regular, None)
            .unwrap()
            .expect("plain grid resolves A");

        assert_ne!(
            overridden.int(),
            regular.int(),
            "font-codepoint-map should change the resolved face"
        );
    }

    #[test]
    fn shared_grid_set_build_grid_honors_disabled_synthetic_styles() {
        let mut cfg = Config::default();
        cfg.font_synthetic_style = FontSyntheticStyle {
            bold: false,
            italic: false,
            bold_italic: false,
        };

        let mut grid = build_grid_from_config(&cfg, 13.0).expect("grid builds");
        let collection = grid.resolver.collection_mut();
        let bold_is_synthetic = collection
            .get_face(Index::new(Style::Bold, 0))
            .unwrap()
            .synthetic_bold_width()
            .is_some();
        let italic_is_synthetic = collection
            .get_face(Index::new(Style::Italic, 0))
            .unwrap()
            .is_skewed();
        let bold_italic = collection
            .get_face(Index::new(Style::BoldItalic, 0))
            .unwrap();
        let bold_italic_is_bold = bold_italic.synthetic_bold_width().is_some();
        let bold_italic_is_italic = bold_italic.is_skewed();

        assert!(!bold_is_synthetic);
        assert!(!italic_is_synthetic);
        assert!(!bold_italic_is_bold);
        assert!(!bold_italic_is_italic);
    }
}

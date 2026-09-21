//! Loads system fonts so titles in any script show real glyphs instead of
//! squares. egui's built-in font only covers Latin, Greek and Cyrillic.
//!
//! We look for well-known fonts by name rather than bundling one: a CJK font is
//! 10-20 MB per language, and every OS already ships suitable ones.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

struct Script {
    name: &'static str,
    /// System font families that cover this script, best first (macOS, then Windows).
    families: &'static [&'static str],
    /// Text the chosen font must be able to draw; used by the tests.
    #[cfg_attr(not(test), allow(dead_code))]
    sample: &'static str,
}

const SCRIPTS: &[Script] = &[
    Script {
        name: "Japanese",
        families: &[
            "Hiragino Sans",
            "Hiragino Kaku Gothic ProN",
            "Yu Gothic UI",
            "Yu Gothic",
            "Meiryo UI",
            "Meiryo",
            "MS Gothic",
            "Noto Sans CJK JP",
            "Noto Sans JP",
            "IPAexGothic",
            "IPAGothic",
            "TakaoGothic",
        ],
        sample: "あいうアイウ日本語",
    },
    Script {
        name: "Chinese (Simplified)",
        families: &[
            "PingFang SC",
            "Hiragino Sans GB",
            "Microsoft YaHei UI",
            "Microsoft YaHei",
            "SimSun",
            "Noto Sans CJK SC",
            "Noto Sans SC",
            "WenQuanYi Micro Hei",
            "WenQuanYi Zen Hei",
            "Droid Sans Fallback",
        ],
        sample: "汉语简体中文",
    },
    Script {
        name: "Chinese (Traditional)",
        families: &[
            "PingFang TC",
            "Microsoft JhengHei UI",
            "Microsoft JhengHei",
            "Noto Sans CJK TC",
            "Noto Sans TC",
        ],
        sample: "漢語繁體中文",
    },
    Script {
        name: "Korean",
        families: &[
            "Apple SD Gothic Neo",
            "Malgun Gothic",
            "Noto Sans CJK KR",
            "Noto Sans KR",
            "NanumGothic",
        ],
        sample: "한국어",
    },
    Script {
        name: "Arabic",
        families: &["Geeza Pro", "Segoe UI", "Arial", "Tahoma", "Noto Sans Arabic", "DejaVu Sans"],
        sample: "العربية",
    },
    Script {
        name: "Hebrew",
        families: &["Arial Hebrew", "Arial", "Segoe UI", "Tahoma", "Noto Sans Hebrew", "DejaVu Sans"],
        sample: "עברית",
    },
    Script {
        name: "Thai",
        families: &["Thonburi", "Leelawadee UI", "Tahoma", "Noto Sans Thai", "Loma", "Garuda"],
        sample: "ภาษาไทย",
    },
    Script {
        name: "Devanagari",
        families: &[
            "Kohinoor Devanagari",
            "Devanagari Sangam MN",
            "Nirmala UI",
            "Mangal",
            "Noto Sans Devanagari",
            "Lohit Devanagari",
        ],
        sample: "हिन्दी",
    },
];

struct Fallback {
    script: &'static str,
    family: &'static str,
    data: &'static [u8],
    index: u32,
}

/// Finds one installed font per script. Scripts with no matching font are skipped.
fn find_fallbacks() -> Vec<Fallback> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    // Several faces often live in one .ttc file; read each file once and share it.
    let mut files: HashMap<PathBuf, &'static [u8]> = HashMap::new();
    let mut used = HashSet::new();
    let mut found = Vec::new();

    for script in SCRIPTS {
        for family in script.families {
            let query = fontdb::Query {
                families: &[fontdb::Family::Name(family)],
                ..Default::default()
            };
            let Some(id) = db.query(&query) else { continue };
            let Some(face) = db.face(id) else { continue };
            let (fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _)) = &face.source
            else {
                continue;
            };
            if !used.insert((path.clone(), face.index)) {
                break; // an earlier script already picked this exact font
            }
            let Some(data) = load_file(&mut files, path) else { continue };
            found.push(Fallback { script: script.name, family, data, index: face.index });
            break;
        }
    }
    found
}

/// Fonts live as long as the app, so leaking the bytes is simpler than
/// juggling lifetimes and lets several faces share one file.
fn load_file(cache: &mut HashMap<PathBuf, &'static [u8]>, path: &PathBuf) -> Option<&'static [u8]> {
    if let Some(data) = cache.get(path) {
        return Some(data);
    }
    let data: &'static [u8] = Box::leak(std::fs::read(path).ok()?.into_boxed_slice());
    cache.insert(path.clone(), data);
    Some(data)
}

/// Adds the system fonts as fallbacks after egui's defaults.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for fallback in find_fallbacks() {
        let key = format!("system-{}-{}", fallback.script, fallback.family);
        let mut data = FontData::from_static(fallback.data);
        data.index = fallback.index;
        fonts.font_data.insert(key.clone(), std::sync::Arc::new(data));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(key.clone());
        }
    }
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever font we pick for a script must really contain that script's
    /// glyphs. Machines without any matching font are skipped, not failed, so
    /// this doesn't break on minimal CI images.
    #[test]
    fn chosen_fonts_cover_their_scripts() {
        let fallbacks = find_fallbacks();
        for fb in &fallbacks {
            let script = SCRIPTS.iter().find(|s| s.name == fb.script).unwrap();
            let face = ttf_parser::Face::parse(fb.data, fb.index).expect("font parses");
            let missing: Vec<char> = script
                .sample
                .chars()
                .filter(|c| face.glyph_index(*c).is_none())
                .collect();
            println!("{:<22} -> {:<28} missing: {missing:?}", fb.script, fb.family);
            // Traditional Chinese fonts and the like can lack a few rare
            // characters; require the script's core sample to be present.
            assert!(missing.is_empty(), "{} lacks {missing:?} for {}", fb.family, fb.script);
        }
    }
}

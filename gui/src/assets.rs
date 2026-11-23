//! Lucide icons besides the ones of gpui-kit (license in `assets/LICENSE-LUCIDE`)
//! et images de l'app

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        &[$((concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg")))),*]
    };
}

const ICONS: &[(&str, &[u8])] = icons![
    "circle-dot",
    "clock",
    "stethoscope",
    "book-open",
    "folder-git-2",
    "git-branch",
    "git-commit-horizontal",
    "git-merge",
    "globe",
    "import",
    "github",
    "house",
    "layers",
    "list",
    "list-checks",
    "message-square",
    "moon",
    "package",
    "pencil",
    "shield-alert",
    "shield-check",
    "sparkles",
    "sun",
    "terminal",
    "trash",
    "undo",
    "redo",
    "upload",
    "variable",
    "workflow",
    "x",
    "zap",
];

const IMAGES: &[(&str, &[u8])] = &[(
    "images/goat.png",
    include_bytes!("../assets/images/goat.png"),
)];

/// Our icons, then the ones of gpui-kit
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().chain(IMAGES).find(|(p, _)| *p == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut list = gpui_kit::assets::Assets.list(path)?;
        list.extend(
            ICONS
                .iter()
                .chain(IMAGES)
                .filter(|(p, _)| p.starts_with(path))
                .map(|(p, _)| SharedString::from(*p)),
        );
        list.sort();
        list.dedup();
        Ok(list)
    }
}

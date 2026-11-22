//! Icônes Lucide en plus de celles de gpui-kit (licence dans `assets/LICENSE-LUCIDE`)

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
    "folder-git-2",
    "git-branch",
    "git-commit-horizontal",
    "git-merge",
    "layers",
    "list-checks",
    "message-square",
    "moon",
    "package",
    "pencil",
    "sparkles",
    "sun",
    "terminal",
    "trash",
    "upload",
    "zap",
];

/// Nos icônes, puis celles de gpui-kit
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().find(|(p, _)| *p == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut list = gpui_kit::assets::Assets.list(path)?;
        list.extend(
            ICONS
                .iter()
                .filter(|(p, _)| p.starts_with(path))
                .map(|(p, _)| SharedString::from(*p)),
        );
        list.sort();
        list.dedup();
        Ok(list)
    }
}

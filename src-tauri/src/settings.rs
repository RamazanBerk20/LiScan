use crate::model::Settings;
use std::fs;
use std::io;
use std::path::PathBuf;

pub fn settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("liscan")
        .join("settings.json")
}

pub fn load_settings() -> Settings {
    let path = settings_path();
    let Ok(data) = fs::read(path) else {
        return Settings::default();
    };
    serde_json::from_slice(&data).unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> io::Result<()> {
    let path = settings_path();
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("settings path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(settings)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(&temporary, data)?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ByteUnitScale, LanguagePreference, ThemePreference};

    #[test]
    fn older_settings_default_to_binary_units_without_losing_preferences() {
        let settings: Settings = serde_json::from_str(
            r#"{
                "theme": "dark",
                "colorScheme": "rainbow",
                "contrast": 84,
                "showSidebar": false,
                "scanOptions": {
                    "crossFilesystems": false,
                    "includeRemoteMounts": false,
                    "includeRemovable": true,
                    "showSmallFiles": false,
                    "exclusions": ["/proc", "/sys", "/dev", "/run"]
                }
            }"#,
        )
        .unwrap();

        assert_eq!(settings.byte_unit_scale, ByteUnitScale::Binary);
        assert_eq!(settings.language, LanguagePreference::System);
        assert!(matches!(settings.theme, ThemePreference::Dark));
        assert_eq!(settings.contrast, 84);
        assert!(!settings.show_sidebar);
    }

    #[test]
    fn decimal_unit_preference_deserializes() {
        let settings: Settings = serde_json::from_str(r#"{"byteUnitScale":"decimal"}"#).unwrap();
        assert_eq!(settings.byte_unit_scale, ByteUnitScale::Decimal);
    }

    #[test]
    fn language_preference_deserializes() {
        let settings: Settings = serde_json::from_str(r#"{"language":"tr"}"#).unwrap();
        assert_eq!(settings.language, LanguagePreference::Tr);
    }
}

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{architecture::ConversionConfig, drawings, text_files};

const CREDENTIAL_SERVICE: &str = "ASCIIFlow Desktop";
const MAX_SETTINGS_BYTES: usize = 16_384;

#[derive(Default, Deserialize, Serialize)]
pub(super) struct AppSettings {
    #[serde(default)]
    pub architecture: ModelSettings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pending_key_removals: Vec<Uuid>,
    #[serde(flatten)]
    other: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub(super) struct ModelSettings {
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub strict_schema: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<SavedKey>,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "store", rename_all = "snake_case")]
pub(super) enum SavedKey {
    CredentialManager { account: Uuid },
    Settings { endpoint: String, api_key: String },
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:11434/v1".into(),
            model: String::new(),
            strict_schema: false,
            key: None,
        }
    }
}

impl AppSettings {
    pub fn load() -> Result<Self, String> {
        let path = settings_path()?;
        if let Some(settings) = read_json(&path)? {
            return Ok(settings);
        }
        // The earlier file held only endpoint metadata. Import it once a user saves settings.
        let mut architecture: ModelSettings = drawings::data_directory()
            .ok()
            .map(|directory| read_json(&directory.join("ai-settings.json")))
            .transpose()?
            .flatten()
            .unwrap_or_default();
        architecture.key = None;
        Ok(Self {
            architecture,
            ..Self::default()
        })
    }

    pub fn save(
        config: ConversionConfig,
        preserve_unread_key: bool,
    ) -> Result<&'static str, String> {
        let has_key = !config.api_key.is_empty();
        if !config.base_url.is_empty() || has_key {
            config.endpoint()?;
        }
        let path = settings_path()?;
        let directory = path.parent().expect("settings path has a parent");
        fs::create_dir_all(directory).map_err(settings_io_error)?;
        let _lock = lock_settings(directory).map_err(settings_io_error)?;
        let mut settings: Self = read_json(&path)?.unwrap_or_default();
        settings
            .pending_key_removals
            .retain(|account| remove_credential(*account).is_err());
        let mut old_key = settings.architecture.key.take();
        let mut status = "App settings saved";
        let key = if !has_key {
            if preserve_unread_key
                && old_key.is_some()
                && config.base_url == settings.architecture.base_url
            {
                status = "App settings saved. The previously stored API key was kept.";
                old_key.take()
            } else {
                None
            }
        } else {
            let account = Uuid::new_v4();
            let value = format!("{}\n{}", config.base_url, config.api_key);
            match credential(account).and_then(|entry| entry.set_password(&value)) {
                Ok(()) => {
                    status = "Settings saved. API key stored in the OS credential manager.";
                    Some(SavedKey::CredentialManager { account })
                }
                Err(error) if credential_manager_unavailable(&error) => {
                    status = "No OS credential manager detected. API key saved in settings.json.";
                    Some(SavedKey::Settings {
                        endpoint: config.base_url.clone(),
                        api_key: config.api_key,
                    })
                }
                Err(_) => {
                    return Err("Could not save the API key in the OS credential manager.".into());
                }
            }
        };
        settings.architecture = ModelSettings {
            base_url: config.base_url,
            model: config.model,
            strict_schema: config.strict_schema,
            key,
        };
        let old_account = credential_account(old_key.as_ref());
        if let Some(account) = old_account {
            settings.pending_key_removals.push(account);
        }
        if let Err(error) = write_settings(&path, &settings) {
            if has_key && let Some(account) = credential_account(settings.architecture.key.as_ref())
            {
                let _ = remove_credential(account);
            }
            return Err(error);
        }
        if let Some(account) = old_account {
            if remove_credential(account).is_ok() {
                settings.pending_key_removals.pop();
                // The saved settings already retain the ID if this cleanup write fails.
                let _ = write_settings(&path, &settings);
            } else {
                status = "Settings saved. The previous credential could not be removed and will be retried on the next save.";
            }
        }
        Ok(status)
    }
}

impl SavedKey {
    pub fn load(self, base_url: &str) -> Result<String, &'static str> {
        match self {
            Self::Settings { endpoint, api_key } if endpoint == base_url => Ok(api_key),
            Self::Settings { .. } => Ok(String::new()),
            Self::CredentialManager { account } => credential(account)
                .and_then(|entry| entry.get_password())
                .map(|value| {
                    value
                        .split_once('\n')
                        .filter(|(endpoint, _)| *endpoint == base_url)
                        .map_or_else(String::new, |(_, key)| key.to_owned())
                })
                .map_err(|_| "Could not read the saved API key from the OS credential manager."),
        }
    }
}

fn settings_path() -> Result<PathBuf, String> {
    let directory = match std::env::var_os("ASCIIFLOW_CONFIG_DIR") {
        Some(path) if Path::new(&path).is_absolute() => PathBuf::from(path),
        Some(_) => return Err("ASCIIFLOW_CONFIG_DIR must be absolute.".into()),
        None => dirs::config_dir()
            .ok_or("The app settings directory is unavailable.")?
            .join(crate::APP_NAME),
    };
    Ok(directory.join("settings.json"))
}

fn lock_settings(directory: &Path) -> io::Result<File> {
    let lock = OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .truncate(false)
        .open(directory.join("settings.lock"))?;
    lock.lock()?;
    Ok(lock)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    match text_files::read_limited(path, MAX_SETTINGS_BYTES) {
        Ok(data) => serde_json::from_slice(&data)
            .map(Some)
            .map_err(|_| format!("Could not parse {}.", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(settings_io_error(error)),
    }
}

fn write_settings(path: &Path, settings: &AppSettings) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(settings).map_err(|_| "Could not encode settings.")?;
    if data.len() > MAX_SETTINGS_BYTES {
        return Err("App settings exceed the file size limit.".into());
    }
    text_files::write_bytes(path, &data).map_err(settings_io_error)
}

fn settings_io_error(error: io::Error) -> String {
    format!("Could not access app settings. {error}")
}

fn credential(account: Uuid) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(CREDENTIAL_SERVICE, &account.to_string())
}

fn credential_account(key: Option<&SavedKey>) -> Option<Uuid> {
    match key {
        Some(SavedKey::CredentialManager { account }) => Some(*account),
        _ => None,
    }
}

fn remove_credential(account: Uuid) -> keyring::Result<()> {
    match credential(account).and_then(|entry| entry.delete_credential()) {
        Err(keyring::Error::NoEntry) => Ok(()),
        result => result,
    }
}

#[cfg(target_os = "linux")]
fn credential_manager_unavailable(error: &keyring::Error) -> bool {
    use dbus_secret_service::Error;

    let keyring::Error::PlatformFailure(error) = error else {
        return false;
    };
    match error.downcast_ref::<Error>() {
        Some(Error::Unavailable) => true,
        Some(Error::Dbus(error)) => matches!(
            error.name(),
            Some(
                "org.freedesktop.DBus.Error.ServiceUnknown"
                    | "org.freedesktop.DBus.Error.NameHasNoOwner"
                    | "org.freedesktop.DBus.Error.NotSupported"
                    | "org.freedesktop.DBus.Error.NoServer"
                    | "org.freedesktop.DBus.Error.FileNotFound"
                    | "org.freedesktop.DBus.Error.Spawn.ExecFailed"
            )
        ),
        _ => false,
    }
}

#[cfg(target_os = "macos")]
fn credential_manager_unavailable(error: &keyring::Error) -> bool {
    let keyring::Error::NoStorageAccess(error) = error else {
        return false;
    };
    error
        .downcast_ref::<security_framework::base::Error>()
        .is_some_and(|error| matches!(error.code(), -25291 | -25294))
}

#[cfg(target_os = "windows")]
fn credential_manager_unavailable(error: &keyring::Error) -> bool {
    matches!(error, keyring::Error::NoStorageAccess(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_fallback_key_is_bound_to_its_endpoint_and_file_is_private() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings: AppSettings = serde_json::from_str(r#"{"other_setting":true}"#).unwrap();
        settings.architecture.key = Some(SavedKey::Settings {
            endpoint: "https://provider.example/v1".into(),
            api_key: "test-only-key".into(),
        });
        write_settings(&path, &settings).unwrap();
        let saved: AppSettings = read_json(&path).unwrap().unwrap();
        assert_eq!(saved.other["other_setting"], true);
        assert_eq!(
            saved
                .architecture
                .key
                .unwrap()
                .load("https://different.example/v1"),
            Ok(String::new()),
        );
        let saved: AppSettings = read_json(&path).unwrap().unwrap();
        assert_eq!(
            saved
                .architecture
                .key
                .unwrap()
                .load("https://provider.example/v1"),
            Ok("test-only-key".into()),
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = path.metadata().unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0);
        }
    }

    #[test]
    fn absent_entry_does_not_trigger_plaintext_fallback() {
        assert!(!credential_manager_unavailable(&keyring::Error::NoEntry));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn missing_secret_service_can_fall_back_but_locked_store_cannot() {
        use dbus_secret_service::Error;

        assert!(credential_manager_unavailable(
            &keyring::Error::PlatformFailure(Box::new(Error::Unavailable))
        ));
        assert!(!credential_manager_unavailable(
            &keyring::Error::NoStorageAccess(Box::new(Error::Locked))
        ));
    }
}

use std::{
    env,
    fs::{File, OpenOptions},
    future::Future,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use gpui::{App, PathPromptOptions};

pub fn open(cx: &mut App) -> impl Future<Output = Result<Option<String>, String>> + use<> {
    let selection = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: None,
    });
    let executor = cx.background_executor().clone();

    async move {
        let paths = selection
            .await
            .map_err(|_| "The open dialog closed unexpectedly.".to_string())?
            .map_err(|error| error.to_string())?;
        let Some(path) = paths.and_then(|paths| paths.into_iter().next()) else {
            return Ok(None);
        };

        executor
            .spawn(async move {
                let bytes = read_limited(&path, crate::canvas::MAX_TEXT_BYTES)
                    .map_err(|error| format!("Could not read {}. {error}", path.display()))?;
                let mut text = String::from_utf8(bytes)
                    .map_err(|_| "The drawing file must contain UTF-8 text.")?;
                if text.starts_with('\u{feff}') {
                    text.drain(..'\u{feff}'.len_utf8());
                }
                Ok(Some(text))
            })
            .await
    }
}

pub(crate) fn open_regular(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NONBLOCK);
    #[cfg(not(unix))]
    if !std::fs::metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Choose a regular file.",
        ));
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Choose a regular file.",
        ));
    }
    Ok(file)
}

pub(crate) fn read_limited(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    open_regular(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("The file exceeds the {limit}-byte limit."),
        ));
    }
    Ok(bytes)
}

pub fn save_generated<F>(
    suggested_name: &'static str,
    generate: F,
    cx: &mut App,
) -> impl Future<Output = Result<Option<PathBuf>, String>> + use<F>
where
    F: FnOnce() -> Result<Vec<u8>, String> + Send + 'static,
{
    let directory = env::home_dir().unwrap_or_else(env::temp_dir);
    let selection = cx.prompt_for_new_path(&directory, Some(suggested_name));
    let executor = cx.background_executor().clone();

    async move {
        let path = selection
            .await
            .map_err(|_| "The save dialog closed unexpectedly.".to_string())?
            .map_err(|error| error.to_string())?;
        let Some(path) = path else {
            return Ok(None);
        };

        executor
            .spawn(async move {
                let data = generate()?;
                write_bytes(&path, &data)
                    .map_err(|error| format!("Could not save {}. {error}", path.display()))?;
                Ok(Some(path))
            })
            .await
    }
}

pub fn write_bytes(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let directory = path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut pending = tempfile::NamedTempFile::new_in(directory)?;
    pending.write_all(data)?;
    pending.as_file().sync_all()?;
    pending.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_replaces_an_existing_file_with_exact_utf8_bytes() -> std::io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("diagram.txt");
        write_bytes(&path, b"A previous diagram with a longer title")?;
        let diagram = "  café\n|---|\n";

        write_bytes(&path, diagram.as_bytes())?;

        assert_eq!(std::fs::read(path)?, diagram.as_bytes());
        Ok(())
    }
}

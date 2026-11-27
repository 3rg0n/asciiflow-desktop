use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::canvas::{Point, Rect};

const FORMAT_VERSION: u32 = 1;
const UNTITLED: &str = "Untitled";

pub type DrawingId = u64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub rect: Rect,
    pub label: Option<String>,
    pub kind: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drawing {
    pub id: DrawingId,
    pub name: String,
    pub cells: Vec<(Point, char)>,
    #[serde(default)]
    pub regions: Vec<Region>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Drawings {
    version: u32,
    active_id: DrawingId,
    next_id: DrawingId,
    drawings: Vec<Drawing>,
}

impl Default for Drawings {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            active_id: 1,
            next_id: 2,
            drawings: vec![Drawing {
                id: 1,
                name: UNTITLED.to_owned(),
                cells: Vec::new(),
                regions: Vec::new(),
            }],
        }
    }
}

impl Drawings {
    pub fn lock() -> io::Result<File> {
        let directory = data_directory()?;
        fs::create_dir_all(&directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("drawings.lock"))?;
        lock.try_lock().map_err(|error| {
            io::Error::other(format!("Cannot lock the drawing library. {error}"))
        })?;
        Ok(lock)
    }

    pub fn load() -> io::Result<Self> {
        Self::load_from(&data_file()?)
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&data_file()?)
    }

    pub fn active_id(&self) -> DrawingId {
        self.active_id
    }

    pub fn active(&self) -> &Drawing {
        self.drawings
            .iter()
            .find(|drawing| drawing.id == self.active_id)
            .expect("the active drawing always exists")
    }

    pub fn active_mut(&mut self) -> &mut Drawing {
        self.drawings
            .iter_mut()
            .find(|drawing| drawing.id == self.active_id)
            .expect("the active drawing always exists")
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Drawing> {
        self.drawings.iter()
    }

    pub fn create(&mut self, name: &str) -> io::Result<DrawingId> {
        let name = drawing_name(name)?;
        let id = self.allocate_id()?;
        self.drawings.push(Drawing {
            id,
            name,
            cells: Vec::new(),
            regions: Vec::new(),
        });
        self.active_id = id;
        Ok(id)
    }

    pub fn rename(&mut self, id: DrawingId, name: &str) -> io::Result<()> {
        let index = self.index(id)?;
        self.drawings[index].name = drawing_name(name)?;
        Ok(())
    }

    pub fn duplicate(&mut self, id: DrawingId, name: &str) -> io::Result<DrawingId> {
        let index = self.index(id)?;
        let name = drawing_name(name)?;
        let id = self.allocate_id()?;
        let mut drawing = self.drawings[index].clone();
        drawing.id = id;
        drawing.name = name;
        self.drawings.insert(index + 1, drawing);
        self.active_id = id;
        Ok(id)
    }

    pub fn delete(&mut self, id: DrawingId) -> io::Result<()> {
        let index = self.index(id)?;
        if self.drawings.len() == 1 {
            self.create(UNTITLED)?;
        }
        self.drawings.remove(index);
        if self.active_id == id {
            self.active_id = self.drawings[index.min(self.drawings.len() - 1)].id;
        }
        Ok(())
    }

    pub fn open(&mut self, id: DrawingId) -> io::Result<()> {
        self.index(id)?;
        self.active_id = id;
        Ok(())
    }

    fn index(&self, id: DrawingId) -> io::Result<usize> {
        self.drawings
            .iter()
            .position(|drawing| drawing.id == id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Drawing no longer exists."))
    }

    fn allocate_id(&mut self) -> io::Result<DrawingId> {
        let id = self.next_id;
        self.next_id = id
            .checked_add(1)
            .ok_or_else(|| io::Error::other("The drawing store has run out of IDs."))?;
        Ok(id)
    }

    fn load_from(path: &Path) -> io::Result<Self> {
        let file = match crate::text_files::open_regular(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        if file.metadata()?.len() > MAX_LIBRARY_BYTES {
            return Err(invalid_data("The drawing library exceeds 32 MiB."));
        }
        let drawings: Self = serde_json::from_reader(BufReader::new(file))?;
        drawings.validate()?;
        Ok(drawings)
    }

    fn save_to(&self, path: &Path) -> io::Result<()> {
        self.validate()?;
        let directory = path
            .parent()
            .filter(|directory| !directory.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(directory)?;
        #[cfg(unix)]
        let directory_handle = File::open(directory)?;
        let mut pending = tempfile::NamedTempFile::new_in(directory)?;
        {
            let mut writer = BufWriter::new(pending.as_file_mut());
            serde_json::to_writer(&mut writer, self)?;
            writer.flush()?;
        }
        if pending.as_file().metadata()?.len() > MAX_LIBRARY_BYTES {
            return Err(invalid_data("The drawing library exceeds 32 MiB."));
        }
        pending.as_file().sync_all()?;
        pending.persist(path)?;
        #[cfg(unix)]
        directory_handle.sync_all()?;
        Ok(())
    }

    fn validate(&self) -> io::Result<()> {
        if self.version != FORMAT_VERSION {
            return Err(invalid_data(
                "This drawing store uses an unsupported format.",
            ));
        }

        let mut ids = BTreeSet::new();
        for drawing in &self.drawings {
            if drawing.id == 0 || drawing.id >= self.next_id || !ids.insert(drawing.id) {
                return Err(invalid_data("The drawing store contains an invalid ID."));
            }
            if drawing.name.trim().is_empty() {
                return Err(invalid_data("The drawing store contains an empty name."));
            }
            let mut coordinates = BTreeSet::new();
            if drawing
                .cells
                .iter()
                .any(|(point, ch)| ch.is_control() || !coordinates.insert(*point))
            {
                return Err(invalid_data("The drawing store contains an invalid cell."));
            }
            for region in &drawing.regions {
                let Rect { min, max } = region.rect;
                if min.x > max.x || min.y > max.y {
                    return Err(invalid_data(
                        "The drawing store contains an invalid region.",
                    ));
                }
            }
        }
        if !ids.contains(&self.active_id) {
            return Err(invalid_data(
                "The active drawing is missing from the store.",
            ));
        }
        Ok(())
    }
}

fn data_file() -> io::Result<PathBuf> {
    Ok(data_directory()?.join("drawings.json"))
}

const MAX_LIBRARY_BYTES: u64 = 32 * 1024 * 1024;

pub fn data_directory() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("ASCIIFLOW_DATA_DIR") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ASCIIFLOW_DATA_DIR must be absolute.",
            ));
        }
        return Ok(path);
    }
    dirs::data_local_dir()
        .map(|directory| directory.join("ASCIIFlow Desktop"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "The local data directory is unavailable.",
            )
        })
}

fn drawing_name(name: &str) -> io::Result<String> {
    let name = name.trim();
    if name.is_empty() {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Enter a drawing name.",
        ))
    } else {
        Ok(name.to_owned())
    }
}

fn invalid_data(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawings_restore_exact_sparse_coordinates_metadata_and_active_id() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("drawings.json");
        let mut drawings = Drawings::load_from(&path)?;
        let first = drawings.active_id();
        drawings.rename(first, "Process notes")?;
        drawings.active_mut().cells = vec![(Point::new(-12, 7), '|'), (Point::new(3, -9), 'é')];
        drawings.active_mut().regions = vec![Region {
            rect: Rect::new(Point::new(-12, -9), Point::new(3, 7)),
            label: Some("Input".to_owned()),
            kind: Some("Flowchart".to_owned()),
        }];
        drawings.save_to(&path)?;

        let second = drawings.create("Another drawing")?;
        drawings.active_mut().cells = vec![(Point::new(40, 42), '+')];
        drawings.save_to(&path)?;

        let mut restored = Drawings::load_from(&path)?;
        assert_eq!(restored.active_id(), second);
        assert!(restored.iter().eq(drawings.iter()));
        restored.open(first)?;
        assert_eq!(restored.active().name, "Process notes");
        assert_eq!(restored.active().cells[0], (Point::new(-12, 7), '|'));
        Ok(())
    }

    #[test]
    fn duplicate_copies_content_and_delete_keeps_an_active_drawing() -> io::Result<()> {
        let mut drawings = Drawings::default();
        let original = drawings.active_id();
        drawings.active_mut().cells.push((Point::new(-1, -2), 'x'));
        let duplicate = drawings.duplicate(original, "Copy")?;
        assert_eq!(drawings.active_id(), duplicate);
        assert_eq!(drawings.active().cells, vec![(Point::new(-1, -2), 'x')]);
        drawings.active_mut().cells.clear();
        drawings.delete(duplicate)?;
        assert_eq!(drawings.active_id(), original);
        assert_eq!(drawings.active().cells, vec![(Point::new(-1, -2), 'x')]);
        drawings.delete(original)?;
        assert_ne!(drawings.active_id(), original);
        assert_eq!(drawings.active().name, UNTITLED);
        assert!(drawings.active().cells.is_empty());
        Ok(())
    }

    #[test]
    fn invalid_regions_are_rejected_without_overwriting_the_last_save() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("drawings.json");
        let mut drawings = Drawings::default();
        drawings.save_to(&path)?;
        let valid = fs::read(&path)?;
        drawings.active_mut().regions.push(Region {
            rect: Rect {
                min: Point::new(1, 0),
                max: Point::new(0, 0),
            },
            label: None,
            kind: None,
        });
        assert_eq!(
            drawings.save_to(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(&path)?, valid);

        fs::write(&path, serde_json::to_vec(&drawings)?)?;
        assert_eq!(
            Drawings::load_from(&path).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        Ok(())
    }
}

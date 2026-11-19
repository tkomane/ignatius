//! Atomic local storage for the plan history catalog.

use super::{MAX_OBSERVATIONS, MAX_SERIES, Observation, Series, valid_series_name};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::platform::{create_private_file, restrict_to_owner, write_atomic};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const VERSION: u32 = 1;
const MAX_CATALOG_BYTES: u64 = 2 * 1024 * 1024;
const CATALOG_FILE: &str = "timeline.json";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Catalog {
    version: u32,
    series: BTreeMap<String, Series>,
}

impl Catalog {
    pub(crate) fn get(&self, name: &str) -> Option<&Series> {
        self.series.get(name)
    }

    pub(crate) fn check_binding(&self, name: &str, binding: &str) -> Result<(), Diagnostic> {
        if let Some(series) = self.get(name) {
            if series.binding != binding {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "the series belongs to another query or connection scope",
                    "checking plan history identity",
                )
                .next_action("use another series name or the original query, schema and route"));
            }
        } else if self.series.len() >= MAX_SERIES {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("plan history already contains {MAX_SERIES} series"),
                "checking plan history capacity",
            )
            .next_action("forget a series you no longer need"));
        }
        Ok(())
    }

    fn empty() -> Self {
        Self {
            version: VERSION,
            series: BTreeMap::new(),
        }
    }

    fn valid(&self) -> bool {
        self.version == VERSION
            && self.series.len() <= MAX_SERIES
            && self
                .series
                .iter()
                .all(|(name, series)| valid_series_name(name) && series.valid())
    }
}

#[derive(Debug)]
pub(crate) struct Store {
    dir: PathBuf,
}

struct CatalogLock(File);

impl Drop for CatalogLock {
    fn drop(&mut self) {
        // A forked child can still hold a reference to the open file until exec.
        let _ = self.0.unlock();
    }
}

impl Store {
    pub(crate) fn new(data_dir: &Path) -> Self {
        Self {
            dir: data_dir.join("plan-history"),
        }
    }

    pub(crate) fn read(&self) -> Result<Catalog, Diagnostic> {
        if !self.check_directory()? {
            return Ok(Catalog::empty());
        }
        let Some(file) = open_catalog(&self.dir.join(CATALOG_FILE))? else {
            return Ok(Catalog::empty());
        };
        let mut bytes = Vec::new();
        file.take(MAX_CATALOG_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(local_io)?;
        if bytes.len() as u64 > MAX_CATALOG_BYTES {
            return Err(invalid_catalog());
        }
        let catalog: Catalog = serde_json::from_slice(&bytes).map_err(|_| invalid_catalog())?;
        if !catalog.valid() {
            return Err(invalid_catalog());
        }
        Ok(catalog)
    }

    pub(crate) fn append(
        &self,
        name: &str,
        binding: String,
        observation: Observation,
    ) -> Result<Catalog, Diagnostic> {
        let _lock = self.lock()?;
        let mut catalog = self.read()?;
        catalog.check_binding(name, &binding)?;
        let series = catalog
            .series
            .entry(name.to_owned())
            .or_insert_with(|| Series {
                binding,
                forgotten_observations: 0,
                observations: Vec::new(),
            });
        let position = series
            .observations
            .partition_point(|entry| entry.unix_millis <= observation.unix_millis);
        series.observations.insert(position, observation);
        if series.observations.len() > MAX_OBSERVATIONS {
            series.observations.remove(0);
            series.forgotten_observations = series.forgotten_observations.saturating_add(1);
        }
        self.write(&catalog)?;
        Ok(catalog)
    }

    pub(crate) fn forget(&self, name: &str) -> Result<bool, Diagnostic> {
        if !self.check_directory()? {
            return Ok(false);
        }
        let _lock = self.lock()?;
        let mut catalog = self.read()?;
        if catalog.series.remove(name).is_none() {
            return Ok(false);
        }
        self.write(&catalog)?;
        Ok(true)
    }

    fn write(&self, catalog: &Catalog) -> Result<(), Diagnostic> {
        if !catalog.valid() {
            return Err(invalid_catalog());
        }
        let bytes = serde_json::to_vec(catalog).map_err(|_| invalid_catalog())?;
        if bytes.len() as u64 > MAX_CATALOG_BYTES {
            return Err(invalid_catalog());
        }
        write_atomic(&self.dir.join(CATALOG_FILE), &bytes).map_err(local_io)
    }

    fn check_directory(&self) -> Result<bool, Diagnostic> {
        match fs::symlink_metadata(&self.dir) {
            Ok(metadata) if metadata.is_dir() => Ok(true),
            Ok(_) => Err(invalid_catalog()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(local_io(error)),
        }
    }

    fn lock(&self) -> Result<CatalogLock, Diagnostic> {
        fs::create_dir_all(&self.dir).map_err(local_io)?;
        self.check_directory()?;
        restrict_to_owner(&self.dir).map_err(local_io)?;
        let path = self.dir.join("timeline.lock");
        let file = match create_private_file(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&path).map_err(local_io)?.is_file() {
                    return Err(invalid_catalog());
                }
                OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map_err(local_io)?
            }
            Err(error) => return Err(local_io(error)),
        };
        file.try_lock().map_err(|error| {
            let error = io::Error::from(error);
            if error.kind() == io::ErrorKind::WouldBlock {
                Diagnostic::new(
                    DiagnosticKind::Config,
                    "another process is updating plan history",
                    "locking the local plan history catalog",
                )
                .next_action("retry after the other process completes")
            } else {
                local_io(error)
            }
        })?;
        Ok(CatalogLock(file))
    }
}

fn open_catalog(path: &Path) -> Result<Option<File>, Diagnostic> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => File::open(path).map(Some).map_err(local_io),
        Ok(_) => Err(invalid_catalog()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(local_io(error)),
    }
}

fn invalid_catalog() -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Config,
        "the plan history catalog is oversized, malformed or unsupported",
        "checking the local plan history catalog",
    )
    .next_action("back up or move plan-history/timeline.json before trying again")
}

fn local_io(error: io::Error) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Config,
        "could not access the local plan history catalog",
        "reading or writing plan history",
    )
    .technical("I/O kind", format!("{:?}", error.kind()))
}

#[cfg(all(test, unix))]
mod tests;

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use fluxemu_graphics::shader::{ShaderLanguage, ShaderSource};
use naga::ShaderStage;
use scc::HashCache;
use strum::IntoEnumIterator;

use crate::shader::{BUILTIN_SHADERS, ShaderType, builtin_shader, convert, shader_type_by_name};

#[derive(Debug, thiserror::Error)]
pub enum ShaderError {
    #[error("Could not parse {0:?} shader:\n{1}")]
    Parse(ShaderLanguage, String),
    #[error("Shader failed validation: {0}")]
    Validation(String),
    #[error("Shader has no {0:?} entry point")]
    MissingEntryPoint(ShaderStage),
    #[error(transparent)]
    WriteWgsl(#[from] naga::back::wgsl::Error),
    #[error(transparent)]
    WriteGlsl(#[from] naga::back::glsl::Error),
    #[error("{0:?} cannot hold separate vertex and fragment sources in one file")]
    SplitStages(ShaderLanguage),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("No shader named {0:?} exists in any language")]
    NotFound(PathBuf),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ShaderKey {
    language: ShaderLanguage,
    name: PathBuf,
}

pub(crate) struct ShaderScan {
    pub available: Vec<PathBuf>,
    pub source: Option<Arc<ShaderSource>>,
}

pub(crate) struct ShaderManager {
    #[cfg_attr(not(feature = "filesystem"), expect(unused))]
    shader_directory: PathBuf,
    shaders: HashCache<ShaderKey, Arc<ShaderSource>>,
}

impl ShaderManager {
    pub fn new(shader_directory: PathBuf) -> Self {
        let manager = Self {
            shader_directory,
            shaders: HashCache::default(),
        };

        #[cfg(feature = "filesystem")]
        manager.write_builtin_shaders();

        manager
    }

    pub fn clear(&self) {
        self.shaders.clear_sync();
    }

    pub fn scan(&self, language: Option<ShaderLanguage>, selected: &Path) -> ShaderScan {
        ShaderScan {
            available: self.available(),
            source: language.map(|language| self.get_or_default(language, selected)),
        }
    }

    pub fn available(&self) -> Vec<PathBuf> {
        #[cfg_attr(not(feature = "filesystem"), expect(unused_mut))]
        let mut available: BTreeSet<_> = BUILTIN_SHADERS
            .values()
            .flat_map(|shaders| shaders.keys())
            .map(|shader_type| PathBuf::from(<&str>::from(*shader_type)))
            .collect();

        #[cfg(feature = "filesystem")]
        for language in ShaderLanguage::iter() {
            available.extend(self.list_shader_files(language));
        }

        available.into_iter().collect()
    }

    pub fn get_or_default(&self, language: ShaderLanguage, name: &Path) -> Arc<ShaderSource> {
        let default_name = Path::new(<&str>::from(ShaderType::default()));

        self.get(language, name)
            .or_else(|error| {
                tracing::error!("Could not load shader {:?}: {}", name, error);

                self.get(language, default_name)
            })
            .unwrap_or_else(|error| {
                tracing::error!("Could not load the default shader: {}", error);

                Arc::new(ShaderSource::new(Arc::from("")))
            })
    }

    pub fn get(
        &self,
        language: ShaderLanguage,
        name: &Path,
    ) -> Result<Arc<ShaderSource>, ShaderError> {
        let key = ShaderKey {
            language,
            name: name.into(),
        };

        if let Some(shader) = self.shaders.read_sync(&key, |_, shader| shader.clone()) {
            return Ok(shader);
        }

        let shader = Arc::new(self.load(language, name)?);
        let _ = self.shaders.put_sync(key, shader.clone());

        Ok(shader)
    }

    fn load(&self, language: ShaderLanguage, name: &Path) -> Result<ShaderSource, ShaderError> {
        if let Some(shader) = self.read(language, name) {
            return Ok(shader);
        }

        let mut conversion_error = None;

        for source_language in ShaderLanguage::iter().filter(|other| *other != language) {
            let Some(shader) = self.read(source_language, name) else {
                continue;
            };

            match convert(source_language, language, &shader) {
                Ok(converted) => {
                    tracing::info!(
                        "Converted shader {:?} from {:?} to {:?}",
                        name,
                        source_language,
                        language
                    );

                    return Ok(converted);
                }
                Err(error) => {
                    tracing::warn!(
                        "Could not convert shader {:?} from {:?} to {:?}: {}",
                        name,
                        source_language,
                        language,
                        error
                    );

                    conversion_error = Some(error);
                }
            }
        }

        Err(conversion_error.unwrap_or_else(|| ShaderError::NotFound(name.to_path_buf())))
    }

    fn read(&self, language: ShaderLanguage, name: &Path) -> Option<ShaderSource> {
        #[cfg(feature = "filesystem")]
        {
            let path = self.shader_file(language, name);

            match std::fs::read_to_string(&path) {
                Ok(source) => return Some(ShaderSource::new(source.into())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => tracing::error!("Could not read shader {:?}: {}", path, error),
            }
        }

        shader_type_by_name(name).and_then(|shader_type| builtin_shader(language, shader_type))
    }

    #[cfg(feature = "filesystem")]
    fn language_directory(&self, language: ShaderLanguage) -> PathBuf {
        self.shader_directory.join::<&str>(language.into())
    }

    #[cfg(feature = "filesystem")]
    fn shader_file(&self, language: ShaderLanguage, name: &Path) -> PathBuf {
        self.language_directory(language)
            .join(name)
            .with_added_extension::<&str>(language.into())
    }

    #[cfg(feature = "filesystem")]
    fn list_shader_files(&self, language: ShaderLanguage) -> Vec<PathBuf> {
        let directory = self.language_directory(language);
        let extension: &str = language.into();

        match std::fs::read_dir(&directory) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|found| found == extension))
                .filter_map(|path| path.file_stem().map(PathBuf::from))
                .collect(),
            Err(error) => {
                tracing::error!("Could not list shader directory {:?}: {}", directory, error);

                Vec::new()
            }
        }
    }

    #[cfg(feature = "filesystem")]
    fn write_builtin_shaders(&self) {
        for language in ShaderLanguage::iter() {
            let directory = self.language_directory(language);

            if let Err(error) = std::fs::create_dir_all(&directory) {
                tracing::error!(
                    "Could not create shader directory {:?}: {}",
                    directory,
                    error
                );

                continue;
            }

            for shader_type in ShaderType::iter() {
                let name = Path::new(<&str>::from(shader_type));
                let path = self.shader_file(language, name);

                if path.try_exists().unwrap_or(true) {
                    continue;
                }

                if let Err(error) = self.write_shader(language, name, &path) {
                    tracing::error!("Could not write builtin shader {:?}: {}", path, error);
                }
            }
        }
    }

    #[cfg(feature = "filesystem")]
    fn write_shader(
        &self,
        language: ShaderLanguage,
        name: &Path,
        path: &Path,
    ) -> Result<(), ShaderError> {
        use std::io::{ErrorKind, Write};

        use crate::shader::to_file;

        let shader = self.get(language, name)?;
        let contents = to_file(language, &shader)?;

        let result = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut file| file.write_all(contents.as_bytes()));

        match result {
            Err(error) if error.kind() != ErrorKind::AlreadyExists => Err(error.into()),
            _ => Ok(()),
        }
    }
}

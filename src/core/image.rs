use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::{CoreError, Result};

/// Container formats commonly shipped by Microsoft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    /// `.iso` optical image.
    Iso,
    /// `.wim` Windows Imaging Format.
    Wim,
    /// `.esd` electronic software download (compressed WIM).
    Esd,
    /// Anything else; the stub rejects these for now.
    Unknown,
}

impl ImageFormat {
    #[must_use]
    pub fn from_extension(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("iso") => Self::Iso,
            Some("wim") => Self::Wim,
            Some("esd") => Self::Esd,
            _ => Self::Unknown,
        }
    }
}

/// Windows build numbers extracted from the image metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsVersion {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
}

impl WindowsVersion {
    /// Whether Windows To Go still works on this build.
    ///
    /// WTG was removed from Windows 10 2004+ and is broken again from Windows 11
    /// 24H2 (build 26100) onwards, where it boot-loops during OOBE.
    #[must_use]
    pub fn wtg_supported(self) -> bool {
        self.build < 26_100
    }
}

/// A single installable edition inside a WIM/ESD container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageEdition {
    /// 1-based index used by `wimlib-imagex apply`.
    pub index: u32,
    pub name: String,
    pub description: Option<String>,
    /// e.g. `x86_64`.
    pub architecture: Option<String>,
    /// Uncompressed size of this image, when known.
    pub size_bytes: Option<u64>,
    /// Windows version reported in the image's `<WINDOWS><VERSION>` block.
    #[serde(default)]
    pub version: Option<WindowsVersion>,
}

/// Metadata extracted from a Windows image.
///
/// The real implementation reads this from `install.wim`/`install.esd` with
/// `wimlib-imagex info --xml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsImage {
    pub path: PathBuf,
    pub format: ImageFormat,
    pub size_bytes: u64,
    /// Selected edition, e.g. `Windows 11 Pro`.
    pub edition: Option<String>,
    /// e.g. `x86_64`.
    pub architecture: Option<String>,
    pub edition_index: u32,
    /// Windows version of the selected edition, when known.
    #[serde(default)]
    pub version: Option<WindowsVersion>,
    /// All editions found in the container.
    #[serde(default)]
    pub editions: Vec<ImageEdition>,
}

impl WindowsImage {
    #[must_use]
    pub fn size_human(&self) -> String {
        super::format::human_size(self.size_bytes)
    }

    /// Select an edition by its 1-based index, updating the summary fields.
    #[must_use]
    pub fn with_edition(mut self, index: u32) -> Self {
        if let Some(edition) = self.editions.iter().find(|edition| edition.index == index) {
            self.edition = Some(edition.name.clone());
            self.architecture = edition.architecture.clone();
            self.version = edition.version;
            self.edition_index = index;
        }
        self
    }

    /// Windows To Go is known not to boot when the build is 24H2 (26100) or newer.
    #[must_use]
    pub fn is_wtg_unsupported(&self) -> bool {
        self.version.is_some_and(|version| !version.wtg_supported())
    }
}

/// Inspect Windows installation media.
pub trait ImageService: Send + Sync {
    fn inspect(&self, path: &Path) -> Result<WindowsImage>;
}

pub(crate) fn validate_extension(path: &Path) -> Result<ImageFormat> {
    let format = ImageFormat::from_extension(path);
    if format == ImageFormat::Unknown {
        return Err(CoreError::UnsupportedImage(path.display().to_string()));
    }
    Ok(format)
}

/// Map the numeric `<ARCH>` value from wimlib's XML to a readable name.
#[must_use]
pub fn architecture_name(code: &str) -> String {
    match code.trim() {
        "0" => "x86".to_owned(),
        "5" => "ARM".to_owned(),
        "6" => "ia64".to_owned(),
        "9" => "x86_64".to_owned(),
        "12" => "ARM64".to_owned(),
        other => other.to_owned(),
    }
}

/// Parse the XML produced by `wimlib-imagex info --xml`.
pub fn parse_wim_info_xml(xml: &str) -> Result<Vec<ImageEdition>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    #[derive(Clone, Copy)]
    enum Field {
        Name,
        Description,
        Size,
        Arch,
        Major,
        Minor,
        Build,
    }

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut editions = Vec::new();
    let mut current: Option<ImageEdition> = None;
    let mut in_windows = false;
    let mut in_version = false;
    let mut version: Option<WindowsVersion> = None;
    let mut field: Option<Field> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => match element.name().as_ref() {
                "IMAGE" => {
                    let index = element
                        .attributes()
                        .flatten()
                        .find(|attribute| attribute.key.as_ref() == "INDEX")
                        .map(|attribute| attribute.value.into_owned())
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(0);
                    current = Some(ImageEdition {
                        index,
                        name: String::new(),
                        description: None,
                        architecture: None,
                        size_bytes: None,
                        version: None,
                    });
                }
                "WINDOWS" => in_windows = true,
                "VERSION" if in_windows => {
                    in_version = true;
                    version = Some(WindowsVersion {
                        major: 0,
                        minor: 0,
                        build: 0,
                    });
                }
                "NAME" if current.is_some() => field = Some(Field::Name),
                "DESCRIPTION" if current.is_some() => field = Some(Field::Description),
                "TOTALBYTES" if current.is_some() => field = Some(Field::Size),
                "ARCH" if in_windows => field = Some(Field::Arch),
                "MAJOR" if in_version => field = Some(Field::Major),
                "MINOR" if in_version => field = Some(Field::Minor),
                "BUILD" if in_version => field = Some(Field::Build),
                _ => {}
            },
            Ok(Event::Text(text)) => {
                if let (Some(field), Some(image)) = (field, current.as_mut()) {
                    let value = text.as_ref().to_owned();
                    let number = value.trim().parse::<u32>().unwrap_or(0);
                    match field {
                        Field::Name => image.name = value,
                        Field::Description => image.description = Some(value),
                        Field::Size => image.size_bytes = value.trim().parse().ok(),
                        Field::Arch => image.architecture = Some(architecture_name(&value)),
                        Field::Major => {
                            if let Some(parsed) = version.as_mut() {
                                parsed.major = number;
                            }
                        }
                        Field::Minor => {
                            if let Some(parsed) = version.as_mut() {
                                parsed.minor = number;
                            }
                        }
                        Field::Build => {
                            if let Some(parsed) = version.as_mut() {
                                parsed.build = number;
                            }
                        }
                    }
                }
                field = None;
            }
            Ok(Event::End(element)) => match element.name().as_ref() {
                "VERSION" => {
                    in_version = false;
                    if let (Some(image), Some(parsed)) = (current.as_mut(), version.take()) {
                        image.version = Some(parsed);
                    }
                }
                "WINDOWS" => in_windows = false,
                "IMAGE" => {
                    if let Some(mut image) = current.take() {
                        if image.name.is_empty() {
                            if let Some(description) = &image.description {
                                image.name = description.clone();
                            }
                        }
                        editions.push(image);
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(CoreError::Parse(format!("invalid wimlib XML: {error}")));
            }
            _ => {}
        }
    }

    Ok(editions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_known_extensions_case_insensitively() {
        assert_eq!(
            ImageFormat::from_extension(Path::new("a.ISO")),
            ImageFormat::Iso
        );
        assert_eq!(
            ImageFormat::from_extension(Path::new("a.wim")),
            ImageFormat::Wim
        );
        assert_eq!(
            ImageFormat::from_extension(Path::new("a.esd")),
            ImageFormat::Esd
        );
        assert_eq!(
            ImageFormat::from_extension(Path::new("a.zip")),
            ImageFormat::Unknown
        );
    }

    #[test]
    fn rejects_unknown_extension() {
        let err = validate_extension(Path::new("windows.zip")).unwrap_err();
        assert!(matches!(err, CoreError::UnsupportedImage(_)));
    }

    const WIM_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<WIM>
    <TOTALBYTES>6000000000</TOTALBYTES>
    <IMAGE INDEX="1">
        <NAME>Windows 11 Home</NAME>
        <DESCRIPTION>Windows 11 Home</DESCRIPTION>
        <TOTALBYTES>10000000000</TOTALBYTES>
        <WINDOWS>
            <ARCH>9</ARCH>
            <VERSION>
                <MAJOR>10</MAJOR>
                <MINOR>0</MINOR>
                <BUILD>26100</BUILD>
            </VERSION>
        </WINDOWS>
    </IMAGE>
    <IMAGE INDEX="2">
        <NAME>Windows 10 Pro</NAME>
        <TOTALBYTES>11000000000</TOTALBYTES>
        <WINDOWS>
            <ARCH>9</ARCH>
            <VERSION>
                <MAJOR>10</MAJOR>
                <MINOR>0</MINOR>
                <BUILD>19045</BUILD>
            </VERSION>
        </WINDOWS>
    </IMAGE>
</WIM>"#;

    #[test]
    fn parses_windows_build_numbers() {
        let editions = parse_wim_info_xml(WIM_XML).unwrap();
        assert_eq!(editions.len(), 2);
        assert_eq!(
            editions[0].version,
            Some(WindowsVersion {
                major: 10,
                minor: 0,
                build: 26100
            })
        );
        assert_eq!(
            editions[1].version,
            Some(WindowsVersion {
                major: 10,
                minor: 0,
                build: 19045
            })
        );
    }

    #[test]
    fn flags_24h2_as_unsupported() {
        let unsupported = WindowsVersion {
            major: 10,
            minor: 0,
            build: 26100,
        };
        assert!(!unsupported.wtg_supported());

        let supported = WindowsVersion {
            major: 10,
            minor: 0,
            build: 22631,
        };
        assert!(supported.wtg_supported());
    }
}

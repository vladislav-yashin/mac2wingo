//! Tiny dependency-free translation layer.
//!
//! Stage 1 supports English and Russian. New languages are added by extending
//! [`Language`] and the table inside [`I18n::t`].

use serde::{Deserialize, Serialize};

/// A supported UI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    En,
    Ru,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::En, Language::Ru];

    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ru => "ru",
        }
    }

    /// Name of the language written in that language.
    #[must_use]
    pub fn native_name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Ru => "Русский",
        }
    }

    /// Best-effort matching of a BCP-47 locale tag to a supported language.
    #[must_use]
    pub fn from_locale(locale: &str) -> Self {
        let language = locale
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match language.as_str() {
            "ru" | "be" | "uk" | "kk" => Self::Ru,
            _ => Self::En,
        }
    }

    /// Detect the operating system language, falling back to English.
    #[must_use]
    pub fn system() -> Self {
        sys_locale::get_locale()
            .map(|locale| Self::from_locale(&locale))
            .unwrap_or(Self::En)
    }
}

/// Translation helper bound to a language.
#[derive(Debug, Clone, Copy)]
pub struct I18n {
    lang: Language,
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(Language::system())
    }
}

impl I18n {
    #[must_use]
    pub fn new(lang: Language) -> Self {
        Self { lang }
    }

    #[must_use]
    pub fn lang(&self) -> Language {
        self.lang
    }

    pub fn set_lang(&mut self, lang: Language) {
        self.lang = lang;
    }

    /// Look up a translation key; unknown keys are returned verbatim.
    #[must_use]
    pub fn t<'a>(&self, key: &'a str) -> &'a str {
        match key {
            // Application -------------------------------------------------
            "app.title" => "Mac2Win Go",
            "app.subtitle" => match self.lang {
                Language::En => "Create a bootable Windows To Go drive",
                Language::Ru => "Создание загрузочного накопителя Windows To Go",
            },
            "app.admin_notice" => match self.lang {
                Language::En => {
                    "Creating the drive needs administrator rights. macOS will ask for your \
                     password once, before writing begins."
                }
                Language::Ru => {
                    "Для создания накопителя нужны права администратора. macOS один раз запросит \
                     пароль перед началом записи."
                }
            },
            "app.tools_missing" => match self.lang {
                Language::En => "Some helper tools are missing from the app bundle:",
                Language::Ru => "В приложении отсутствуют вспомогательные инструменты:",
            },

            // Navigation ---------------------------------------------------
            "nav.back" => match self.lang {
                Language::En => "Back",
                Language::Ru => "Назад",
            },
            "nav.next" => match self.lang {
                Language::En => "Next",
                Language::Ru => "Далее",
            },
            "nav.start" => match self.lang {
                Language::En => "Start",
                Language::Ru => "Начать",
            },
            "nav.cancel" => match self.lang {
                Language::En => "Cancel",
                Language::Ru => "Отмена",
            },
            "nav.close" => match self.lang {
                Language::En => "Close",
                Language::Ru => "Закрыть",
            },
            "nav.retry" => match self.lang {
                Language::En => "Retry",
                Language::Ru => "Повторить",
            },
            "nav.refresh" => match self.lang {
                Language::En => "Refresh",
                Language::Ru => "Обновить",
            },
            "nav.lang_toggle" => match self.lang {
                Language::En => "Language",
                Language::Ru => "Язык",
            },

            // Steps --------------------------------------------------------
            "step.welcome.title" => match self.lang {
                Language::En => "Welcome",
                Language::Ru => "Добро пожаловать",
            },
            "step.welcome.body" => match self.lang {
                Language::En => {
                    "This wizard writes a full Windows installation to a USB drive so you can \
                     boot it on another computer. All data on the target drive will be erased."
                }
                Language::Ru => {
                    "Мастер запишет полноценную систему Windows на USB-накопитель, чтобы \
                     запускать её на другом компьютере. Все данные на целевом диске будут \
                     уничтожены."
                }
            },
            "step.image.title" => match self.lang {
                Language::En => "Windows image",
                Language::Ru => "Образ Windows",
            },
            "step.image.body" => match self.lang {
                Language::En => "Pick a Windows ISO, install.wim or install.esd file.",
                Language::Ru => "Выберите файл Windows ISO, install.wim или install.esd.",
            },
            "step.image.choose" => match self.lang {
                Language::En => "Choose image…",
                Language::Ru => "Выбрать образ…",
            },
            "step.image.selected" => match self.lang {
                Language::En => "Selected image",
                Language::Ru => "Выбранный образ",
            },
            "step.image.none" => match self.lang {
                Language::En => "No image selected",
                Language::Ru => "Образ не выбран",
            },
            "step.image.edition" => match self.lang {
                Language::En => "Edition",
                Language::Ru => "Редакция",
            },
            "step.device.title" => match self.lang {
                Language::En => "Target drive",
                Language::Ru => "Целевой диск",
            },
            "step.device.body" => match self.lang {
                Language::En => "Only external USB or Thunderbolt drives are listed.",
                Language::Ru => "Показаны только внешние USB- и Thunderbolt-накопители.",
            },
            "step.device.empty" => match self.lang {
                Language::En => "No eligible external drives found.",
                Language::Ru => "Подходящие внешние накопители не найдены.",
            },
            "step.device.scan" => match self.lang {
                Language::En => "Scan drives",
                Language::Ru => "Найти диски",
            },
            "step.confirm.title" => match self.lang {
                Language::En => "Confirm erase",
                Language::Ru => "Подтвердите очистку",
            },
            "step.confirm.warning" => match self.lang {
                Language::En => "All data on the selected drive will be destroyed",
                Language::Ru => "Все данные на выбранном диске будут уничтожены",
            },
            "step.confirm.wtg_unsupported" => match self.lang {
                Language::En => {
                    "This Windows build (11 24H2 / 26100 or newer) does not support Windows To \
                     Go and will boot-loop during setup. Use Windows 10 22H2 or Windows 11 23H2."
                }
                Language::Ru => {
                    "Эта сборка Windows (11 24H2 / 26100 и новее) не поддерживает Windows To Go \
                     и уйдёт в цикл перезагрузки при настройке. Используйте Windows 10 22H2 или \
                     Windows 11 23H2."
                }
            },
            "step.options.title" => match self.lang {
                Language::En => "Options",
                Language::Ru => "Параметры",
            },
            "step.progress.title" => match self.lang {
                Language::En => "Writing",
                Language::Ru => "Запись",
            },
            "step.done.title" => match self.lang {
                Language::En => "Done",
                Language::Ru => "Готово",
            },
            "step.done.body" => match self.lang {
                Language::En => "The drive is ready. You can boot from it on the target computer.",
                Language::Ru => "Накопитель готов. Можно загружаться с него на целевом компьютере.",
            },
            "step.done.again" => match self.lang {
                Language::En => "Create another",
                Language::Ru => "Создать ещё один",
            },

            // Fields -------------------------------------------------------
            "label.name" => match self.lang {
                Language::En => "Name",
                Language::Ru => "Название",
            },
            "label.size" => match self.lang {
                Language::En => "Size",
                Language::Ru => "Размер",
            },
            "label.bus" => match self.lang {
                Language::En => "Connection",
                Language::Ru => "Подключение",
            },
            "label.scheme" => match self.lang {
                Language::En => "Partition map",
                Language::Ru => "Таблица разделов",
            },
            "label.volumes" => match self.lang {
                Language::En => "Volumes",
                Language::Ru => "Разделы",
            },
            "label.mount" => match self.lang {
                Language::En => "Mount point",
                Language::Ru => "Точка монтирования",
            },
            "label.format" => match self.lang {
                Language::En => "Format",
                Language::Ru => "Формат",
            },
            "label.edition" => match self.lang {
                Language::En => "Edition",
                Language::Ru => "Редакция",
            },
            "label.architecture" => match self.lang {
                Language::En => "Architecture",
                Language::Ru => "Архитектура",
            },
            "label.version" => match self.lang {
                Language::En => "Version",
                Language::Ru => "Версия",
            },
            "label.partition_scheme" => match self.lang {
                Language::En => "Partition scheme",
                Language::Ru => "Схема разделов",
            },
            "label.drivers" => match self.lang {
                Language::En => "Steam drivers",
                Language::Ru => "Драйверы Steam",
            },

            // Bus ----------------------------------------------------------
            "bus.usb" => "USB",
            "bus.thunderbolt" => "Thunderbolt",
            "bus.firewire" => "FireWire",
            "bus.disk_image" => match self.lang {
                Language::En => "Disk image",
                Language::Ru => "Образ диска",
            },
            "bus.internal" => match self.lang {
                Language::En => "Internal",
                Language::Ru => "Внутренний",
            },
            "bus.unknown" => match self.lang {
                Language::En => "Unknown",
                Language::Ru => "Неизвестно",
            },

            // Options ------------------------------------------------------
            "options.uefi" => "UEFI",
            "options.uefi_hint" => match self.lang {
                Language::En => "Install UEFI boot files (recommended for modern PCs).",
                Language::Ru => "Установить UEFI-загрузчик (рекомендуется для современных ПК).",
            },
            "options.skip_size_check" => match self.lang {
                Language::En => "Skip size check",
                Language::Ru => "Пропустить проверку размера",
            },
            "options.skip_size_check_hint" => match self.lang {
                Language::En => "Allow writing to drives smaller than the safe minimum.",
                Language::Ru => "Разрешить запись на диски меньше безопасного минимума.",
            },
            "options.verify" => match self.lang {
                Language::En => "Verify after write",
                Language::Ru => "Проверять после записи",
            },
            "options.verify_hint" => match self.lang {
                Language::En => "Re-read the drive and compare checksums. Very slow.",
                Language::Ru => "Перечитать диск и сравнить контрольные суммы. Очень медленно.",
            },
            "options.gpt" => "GPT",
            "options.mbr" => "MBR",
            "options.ntfs" => "NTFS",
            "options.local_account" => match self.lang {
                Language::En => "Create a local administrator account",
                Language::Ru => "Создать локальную учётную запись администратора",
            },
            "options.local_account_hint" => match self.lang {
                Language::En => {
                    "Skips the Microsoft account requirement and creates a local account during \
                     setup. The password may be left empty."
                }
                Language::Ru => {
                    "Пропускает требование аккаунта Microsoft и создаёт локальную учётную запись \
                     при настройке. Пароль можно оставить пустым."
                }
            },
            "options.local_username" => match self.lang {
                Language::En => "Username",
                Language::Ru => "Имя пользователя",
            },
            "options.local_password" => match self.lang {
                Language::En => "Password",
                Language::Ru => "Пароль",
            },
            "options.drivers" => match self.lang {
                Language::En => "Steam hardware drivers",
                Language::Ru => "Драйверы Steam-устройств",
            },
            "options.drivers_hint" => match self.lang {
                Language::En => {
                    "Drivers are staged on the drive and installed automatically on first \
                     logon. Choose a model to download the official Valve packages, or select \
                     ZIP archives/folders yourself."
                }
                Language::Ru => {
                    "Драйверы копируются на диск и устанавливаются автоматически при первом \
                     входе. Выберите модель, чтобы скачать официальные пакеты Valve, или \
                     укажите ZIP-архивы/папки самостоятельно."
                }
            },
            "options.drivers.none" => match self.lang {
                Language::En => "Do not add drivers",
                Language::Ru => "Не добавлять драйверы",
            },
            "options.drivers.lcd" => match self.lang {
                Language::En => "Steam Deck LCD",
                Language::Ru => "Steam Deck LCD",
            },
            "options.drivers.oled" => match self.lang {
                Language::En => "Steam Deck OLED",
                Language::Ru => "Steam Deck OLED",
            },
            "options.drivers.machine" => match self.lang {
                Language::En => "Steam Machine",
                Language::Ru => "Steam Machine",
            },
            "options.drivers.local" => match self.lang {
                Language::En => "From ZIP files or folders…",
                Language::Ru => "Из ZIP-файлов или папок…",
            },
            "options.drivers.add_files" => match self.lang {
                Language::En => "Add ZIP files…",
                Language::Ru => "Добавить ZIP-файлы…",
            },
            "options.drivers.add_folder" => match self.lang {
                Language::En => "Add folder…",
                Language::Ru => "Добавить папку…",
            },
            "options.drivers_x86_only" => match self.lang {
                Language::En => {
                    "Steam Deck and Steam Machine drivers are x86_64 only and cannot be added \
                     to this image."
                }
                Language::Ru => {
                    "Драйверы Steam Deck и Steam Machine существуют только для x86_64 и не \
                     могут быть добавлены в этот образ."
                }
            },

            // Stages -------------------------------------------------------
            "stage.preparing_drivers" => match self.lang {
                Language::En => "Staging Steam drivers",
                Language::Ru => "Подготовка драйверов Steam",
            },
            "stage.preparing" => match self.lang {
                Language::En => "Preparing",
                Language::Ru => "Подготовка",
            },
            "stage.partitioning" => match self.lang {
                Language::En => "Partitioning",
                Language::Ru => "Разметка",
            },
            "stage.formatting" => match self.lang {
                Language::En => "Formatting",
                Language::Ru => "Форматирование",
            },
            "stage.applying_image" => match self.lang {
                Language::En => "Applying image",
                Language::Ru => "Применение образа",
            },
            "stage.installing_drivers" => match self.lang {
                Language::En => "Adding Steam drivers",
                Language::Ru => "Добавление драйверов Steam",
            },
            "stage.configuring" => match self.lang {
                Language::En => "Configuring Windows To Go",
                Language::Ru => "Настройка Windows To Go",
            },
            "stage.installing_bootloader" => match self.lang {
                Language::En => "Installing bootloader",
                Language::Ru => "Установка загрузчика",
            },
            "stage.verifying" => match self.lang {
                Language::En => "Verifying",
                Language::Ru => "Проверка",
            },
            "stage.ejecting" => match self.lang {
                Language::En => "Ejecting",
                Language::Ru => "Извлечение",
            },
            "stage.finished" => match self.lang {
                Language::En => "Finished",
                Language::Ru => "Завершено",
            },

            // Statuses -----------------------------------------------------
            "status.ready" => match self.lang {
                Language::En => "Ready",
                Language::Ru => "Готово к работе",
            },
            "status.scanning" => match self.lang {
                Language::En => "Scanning drives…",
                Language::Ru => "Поиск дисков…",
            },
            "status.writing" => match self.lang {
                Language::En => "Writing…",
                Language::Ru => "Запись…",
            },
            "status.done" => match self.lang {
                Language::En => "Completed",
                Language::Ru => "Завершено",
            },
            "status.failed" => match self.lang {
                Language::En => "Failed",
                Language::Ru => "Ошибка",
            },
            "error.title" => match self.lang {
                Language::En => "Something went wrong",
                Language::Ru => "Что-то пошло не так",
            },

            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_detection() {
        assert_eq!(Language::from_locale("ru-RU"), Language::Ru);
        assert_eq!(Language::from_locale("ru"), Language::Ru);
        assert_eq!(Language::from_locale("en-US"), Language::En);
        assert_eq!(Language::from_locale("de-DE"), Language::En);
        assert_eq!(Language::from_locale(""), Language::En);
    }

    #[test]
    fn translations_differ_per_language() {
        let en = I18n::new(Language::En);
        let ru = I18n::new(Language::Ru);
        assert_eq!(en.t("nav.next"), "Next");
        assert_eq!(ru.t("nav.next"), "Далее");
    }

    #[test]
    fn unknown_key_is_returned_verbatim() {
        let i18n = I18n::new(Language::En);
        assert_eq!(i18n.t("does.not.exist"), "does.not.exist");
    }

    #[test]
    fn every_stage_has_a_translation() {
        use crate::core::progress::Stage;
        for lang in Language::ALL {
            let i18n = I18n::new(lang);
            for stage in Stage::ALL {
                let key = stage.key();
                assert_ne!(i18n.t(key), key, "missing translation for {key}");
            }
        }
    }
}

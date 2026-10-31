use std::collections::HashMap;

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    En,
    ZhCn,
    ZhTw,
}

#[derive(Deserialize)]
struct RawDictionaries {
    en: HashMap<String, String>,
    #[serde(rename = "zh-CN")]
    zh_cn: HashMap<String, String>,
    #[serde(rename = "zh-TW")]
    zh_tw: HashMap<String, String>,
}

pub struct I18n {
    dictionaries: RawDictionaries,
    locale: Locale,
}

impl I18n {
    pub fn load(preference: &str) -> Self {
        let dictionaries: RawDictionaries = serde_json::from_str(include_str!("../ui/i18n.json"))
            .expect("bundled i18n.json must be valid");
        Self {
            dictionaries,
            locale: resolve(preference),
        }
    }

    pub fn set_preference(&mut self, preference: &str) {
        self.locale = resolve(preference);
    }

    fn dictionary(&self) -> &HashMap<String, String> {
        match self.locale {
            Locale::En => &self.dictionaries.en,
            Locale::ZhCn => &self.dictionaries.zh_cn,
            Locale::ZhTw => &self.dictionaries.zh_tw,
        }
    }

    pub fn t(&self, key: &str) -> String {
        self.dictionary()
            .get(key)
            .or_else(|| self.dictionaries.en.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    pub fn t_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut text = self.t(key);
        for (name, value) in args {
            text = text.replace(&format!("{{{name}}}"), value);
        }
        text
    }
}

pub fn resolve(preference: &str) -> Locale {
    match preference {
        "en" => Locale::En,
        "zh-CN" => Locale::ZhCn,
        "zh-TW" => Locale::ZhTw,
        _ => detect_system_locale(),
    }
}

pub fn detect_system_locale() -> Locale {
    let locale = sys_locale::get_locale().unwrap_or_default().to_lowercase();

    if locale.starts_with("zh") {
        if locale.contains("tw")
            || locale.contains("hk")
            || locale.contains("mo")
            || locale.contains("hant")
        {
            Locale::ZhTw
        } else {
            Locale::ZhCn
        }
    } else {
        Locale::En
    }
}

/// Index used by the settings UI: 0 = system, 1 = English, 2 = 简体中文,
/// 3 = 繁體中文.
pub fn language_index(preference: &str) -> i32 {
    match preference {
        "en" => 1,
        "zh-CN" => 2,
        "zh-TW" => 3,
        _ => 0,
    }
}

pub fn language_preference(index: i32) -> &'static str {
    match index {
        1 => "en",
        2 => "zh-CN",
        3 => "zh-TW",
        _ => "system",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> RawDictionaries {
        serde_json::from_str(include_str!("../ui/i18n.json")).unwrap()
    }

    #[test]
    fn all_locales_have_the_same_keys() {
        let raw = raw();
        let mut english: Vec<&String> = raw.en.keys().collect();
        let mut chinese: Vec<&String> = raw.zh_cn.keys().collect();
        let mut traditional: Vec<&String> = raw.zh_tw.keys().collect();
        english.sort();
        chinese.sort();
        traditional.sort();
        assert_eq!(english, chinese);
        assert_eq!(english, traditional);
    }

    #[test]
    fn placeholders_are_preserved_across_locales() {
        fn placeholders(value: &str) -> Vec<&str> {
            value
                .split(['{', '}'])
                .skip(1)
                .step_by(2)
                .collect::<Vec<_>>()
        }

        let raw = raw();
        for (key, english) in &raw.en {
            let expected = placeholders(english);
            for dictionary in [&raw.zh_cn, &raw.zh_tw] {
                let value = dictionary.get(key).unwrap();
                assert_eq!(placeholders(value), expected, "key {key}");
            }
        }
    }

    #[test]
    fn preference_mapping_round_trips() {
        for index in 0..4 {
            assert_eq!(language_index(language_preference(index)), index);
        }
    }
}

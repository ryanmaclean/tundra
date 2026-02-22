use fluent_bundle::{FluentBundle, FluentResource, FluentArgs};
use unic_langid::LanguageIdentifier;
use leptos::prelude::*;
use reactive_graph::owner::LocalStorage;
use std::collections::HashMap;

const EN_FTL: &str = include_str!("locales/en.ftl");
const FR_FTL: &str = include_str!("locales/fr.ftl");

/// Supported locales
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Locale {
    En,
    Fr,
}

impl Locale {
    pub fn lang_id(&self) -> LanguageIdentifier {
        match self {
            Locale::En => "en".parse().unwrap(),
            Locale::Fr => "fr".parse().unwrap(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Locale::En => "English",
            Locale::Fr => "Français",
        }
    }

    pub fn all() -> &'static [Locale] {
        &[Locale::En, Locale::Fr]
    }

    fn ftl_source(&self) -> &'static str {
        match self {
            Locale::En => EN_FTL,
            Locale::Fr => FR_FTL,
        }
    }
}

/// Translation store holding Fluent bundles for each locale.
pub struct I18n {
    bundles: HashMap<Locale, FluentBundle<FluentResource>>,
    current: Locale,
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        let mut bundles = HashMap::new();
        for loc in Locale::all() {
            let resource = FluentResource::try_new(loc.ftl_source().to_string())
                .expect("Failed to parse FTL resource");
            let mut bundle = FluentBundle::new(vec![loc.lang_id()]);
            bundle
                .add_resource(resource)
                .expect("Failed to add FTL resource to bundle");
            bundles.insert(*loc, bundle);
        }
        Self {
            bundles,
            current: locale,
        }
    }

    /// Translate a message key using the current locale.
    pub fn t(&self, key: &str) -> String {
        self.t_with_locale(self.current, key)
    }

    /// Translate a message key with arguments using the current locale.
    pub fn t_args(&self, key: &str, args: &FluentArgs) -> String {
        self.t_args_with_locale(self.current, key, args)
    }

    /// Set the active locale.
    pub fn set_locale(&mut self, locale: Locale) {
        self.current = locale;
    }

    /// Get the active locale.
    pub fn current(&self) -> Locale {
        self.current
    }

    fn t_with_locale(&self, locale: Locale, key: &str) -> String {
        let bundle = self.bundles.get(&locale).expect("Missing locale bundle");
        let msg = match bundle.get_message(key) {
            Some(m) => m,
            None => return key.to_string(),
        };
        let pattern = match msg.value() {
            Some(p) => p,
            None => return key.to_string(),
        };
        let mut errors = vec![];
        bundle
            .format_pattern(pattern, None, &mut errors)
            .to_string()
    }

    fn t_args_with_locale(&self, locale: Locale, key: &str, args: &FluentArgs) -> String {
        let bundle = self.bundles.get(&locale).expect("Missing locale bundle");
        let msg = match bundle.get_message(key) {
            Some(m) => m,
            None => return key.to_string(),
        };
        let pattern = match msg.value() {
            Some(p) => p,
            None => return key.to_string(),
        };
        let mut errors = vec![];
        bundle
            .format_pattern(pattern, Some(args), &mut errors)
            .to_string()
    }
}

/// Type alias for the i18n stored value using local (non-Send) storage,
/// since FluentBundle contains RefCell and is not Send+Sync.
/// This is safe in WASM which is single-threaded.
type I18nStore = StoredValue<I18n, LocalStorage>;

/// Provide i18n context to the Leptos component tree.
///
/// Call this once at the top of your `App` component. Downstream components
/// can then use `t("key")` to obtain translated strings.
pub fn provide_i18n() {
    let (locale, set_locale) = signal(Locale::En);
    let i18n: I18nStore = StoredValue::new_local(I18n::new(Locale::En));
    provide_context(locale);
    provide_context(set_locale);
    provide_context(i18n);
}

/// Get a translated string for `key` in the current locale.
///
/// Must be called inside a component tree where `provide_i18n()` has been invoked.
pub fn t(key: &str) -> String {
    let locale: ReadSignal<Locale> = use_context().expect("i18n locale not provided — ensure provide_i18n() is called");
    let i18n: I18nStore = use_context().expect("i18n store not provided — ensure provide_i18n() is called");
    // Read locale to create a reactive dependency so re-renders happen on locale change.
    let current = locale.get();
    i18n.with_value(|i| {
        i.t_with_locale(current, key)
    })
}

/// Get a translated string with interpolation arguments.
pub fn t_args(key: &str, args: &FluentArgs) -> String {
    let locale: ReadSignal<Locale> = use_context().expect("i18n locale not provided — ensure provide_i18n() is called");
    let i18n: I18nStore = use_context().expect("i18n store not provided — ensure provide_i18n() is called");
    let current = locale.get();
    i18n.with_value(|i| {
        i.t_args_with_locale(current, key, args)
    })
}

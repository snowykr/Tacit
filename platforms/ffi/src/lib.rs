//! C interface of the input method, for the macOS and Linux text services. Same contract as the
//! core bridge (`tacit_core::session::bridge`): a key and optional text before the caret go in,
//! a string comes out — `1` if the key was consumed (else `0`), then each command preceded by
//! `\u{1}`: `M<text>` set the composing text, `I<text>` commit it (replacing the composing
//! text), `B<n>` delete n UTF-16 units before the caret. Free a returned string with
//! `ts_string_free`. Nothing here unwinds into the caller.

use std::ffi::{c_char, c_int, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Mutex;

use tacit_core::session::bridge::Ime;
use tacit_core::store::policy::Override;
use tacit_core::{make_engine, AppPolicies, InputSession, LearningEngine, Settings};
#[cfg(feature = "japanese")]
use tacit_core::{DictionaryConverter, Language};

/// The instance behind a handle from `ts_new`.
pub struct Handle(Mutex<State>);

struct State {
    ime: Ime,
    dir: PathBuf,
    stamps: [Option<std::time::SystemTime>; 3],
}

fn stamps(dir: &std::path::Path) -> [Option<std::time::SystemTime>; 3] {
    ["settings.json", "apps.json", "lexicon.json"].map(|name| {
        std::fs::metadata(dir.join(name))
            .and_then(|m| m.modified())
            .ok()
    })
}

impl State {
    fn activate(&mut self, app: &str) {
        let fresh = stamps(&self.dir);
        if fresh != self.stamps {
            self.ime.reset();
            self.ime = Ime::with_session(session_in(&self.dir));
            self.stamps = fresh;
        }
        self.ime.activate(app);
    }
}

fn text(s: *const c_char) -> String {
    if s.is_null() {
        return String::new();
    }
    // SAFETY: the caller passes a NUL-terminated string that stays valid for this call.
    unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
}

fn optional_text(s: *const c_char) -> Option<String> {
    (!s.is_null()).then(|| text(s))
}

fn with<R>(h: *mut Handle, fallback: R, f: impl FnOnce(&mut State) -> R) -> R {
    if h.is_null() {
        return fallback;
    }
    // SAFETY: `h` comes from `ts_new` and is used until `ts_free`.
    let handle = unsafe { &*h };
    catch_unwind(AssertUnwindSafe(|| {
        f(&mut handle.0.lock().unwrap_or_else(|e| e.into_inner()))
    }))
    .unwrap_or(fallback)
}

fn reply(s: String) -> *mut c_char {
    CString::new(s.replace('\0', "")).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// The typing session the way the Windows text service builds it: the languages from
/// settings.json whose packs are installed, the app rules, the kana→kanji converter. With no
/// language available every key goes through as typed.
fn session_in(dir: &std::path::Path) -> InputSession {
    let settings = Settings::load(Some(&dir.join("settings.json")));
    tacit_core::pack::refresh_for(tacit_core::pack_dirs(), &settings.languages);
    let mut policies = AppPolicies::load(Some(&dir.join("apps.json")));
    let lexicon = Some(dir.join("lexicon.json"));
    let (_languages, engine) = match settings.languages.available() {
        Some(languages) => {
            let engine: std::sync::Arc<LearningEngine> = make_engine(lexicon, &languages);
            (languages, engine)
        }
        None => {
            policies = AppPolicies::new([(
                AppPolicies::DEFAULT_KEY.to_string(),
                Override {
                    enabled: Some(false),
                    ..Override::default()
                },
            )]);
            let languages = tacit_core::Languages::legacy(false);
            (languages.clone(), make_engine(None, &languages))
        }
    };
    #[allow(unused_mut)]
    let mut session = InputSession::new(engine, policies, settings.return_confirms_only);
    #[cfg(feature = "japanese")]
    if _languages.contains(Language::Japanese) && tacit_core::pack::available(Language::Japanese) {
        let learning = Some(dir.join("kanji_learning.json"));
        session = session.with_japanese(
            std::sync::Arc::new(DictionaryConverter::new(learning)),
            settings.live_conversion,
        );
    }
    #[cfg(feature = "japanese")]
    session.set_full_width_digits(settings.full_width_digits);
    #[cfg(feature = "japanese")]
    session.set_full_width_space(settings.full_width_space);
    session
}

/// A new input method over the settings, apps and lexicon in `dir` (NULL or "": the user's
/// usual folder). NULL on failure.
#[no_mangle]
pub extern "C" fn ts_new(dir: *const c_char) -> *mut Handle {
    let dir = Some(text(dir))
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(tacit_core::support_dir);
    let Some(dir) = dir else {
        return std::ptr::null_mut();
    };
    catch_unwind(|| {
        let ime = Ime::with_session(session_in(&dir));
        let state = State {
            ime,
            stamps: stamps(&dir),
            dir,
        };
        Box::into_raw(Box::new(Handle(Mutex::new(state))))
    })
    .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
/// # Safety
/// `h` is NULL or from `ts_new`, and is not used afterwards.
pub unsafe extern "C" fn ts_free(h: *mut Handle) {
    if !h.is_null() {
        // SAFETY: the handle is not used after this call.
        drop(unsafe { Box::from_raw(h) });
    }
}

#[no_mangle]
/// # Safety
/// `s` is NULL or a string this library returned, and is not used afterwards.
pub unsafe extern "C" fn ts_string_free(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: `s` came from this library's `reply`.
        drop(unsafe { CString::from_raw(s) });
    }
}

/// A new text field in the program `app` (picks its rule; NULL or "" for unknown).
#[no_mangle]
pub extern "C" fn ts_activate(h: *mut Handle, app: *const c_char) {
    let app = text(app);
    with(h, (), |state| state.activate(&app));
}

/// `kind`: 0 letter, 1 printable, 2 delete, 3 escape, 4 cycle, 5 enter, 6 tab, else other.
#[no_mangle]
pub extern "C" fn ts_key(
    h: *mut Handle,
    kind: c_int,
    ch: u32,
    before: *const c_char,
) -> *mut c_char {
    let (ch, before) = (char::from_u32(ch).unwrap_or('\0'), optional_text(before));
    reply(with(h, "0".to_string(), |state| {
        state.ime.key(kind, ch, before.as_deref())
    }))
}

#[no_mangle]
pub extern "C" fn ts_reset(h: *mut Handle) {
    with(h, (), |state| state.ime.reset());
}

#[no_mangle]
pub extern "C" fn ts_toggle_pause(h: *mut Handle, before: *const c_char) -> *mut c_char {
    let before = optional_text(before);
    reply(with(h, "0".to_string(), |state| {
        state.ime.toggle_pause(before.as_deref())
    }))
}

#[no_mangle]
pub extern "C" fn ts_paused(h: *mut Handle) -> c_int {
    with(h, false, |state| state.ime.paused()) as c_int
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(p: *mut c_char) -> String {
        // SAFETY: `p` is a string this library just returned.
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { ts_string_free(p) };
        s
    }

    fn call(h: *mut Handle, kind: c_int, ch: char, before: Option<&str>) -> String {
        let before = before.map(|before| CString::new(before).unwrap());
        let before = before
            .as_ref()
            .map_or(std::ptr::null(), |before| before.as_ptr());
        result(ts_key(h, kind, ch as u32, before))
    }

    fn new_handle(tag: &str) -> (std::path::PathBuf, *mut Handle) {
        let dir = std::env::temp_dir().join(format!("tacit-ffi-{tag}-{}", std::process::id()));
        let dir_c = CString::new(dir.to_string_lossy().as_bytes()).unwrap();
        let h = ts_new(dir_c.as_ptr());
        assert!(!h.is_null());
        ts_activate(h, c"notepad".as_ptr());
        (dir, h)
    }

    fn type_without_context(
        h: *mut Handle,
        field: &mut String,
        composing: &mut String,
        keys: &str,
    ) {
        for ch in keys.chars() {
            let out = call(h, 0, ch, None);
            assert!(out.starts_with('1'), "{out:?}");
            apply(field, composing, &out);
        }
    }

    /// Plays the commands onto the text the way a text field would.
    fn apply(field: &mut String, composing: &mut String, out: &str) {
        for op in out.split('\u{1}').skip(1) {
            let (tag, rest) = op.split_at(1);
            match tag {
                "M" | "I" => {
                    field.truncate(field.len() - composing.len());
                    field.push_str(rest);
                    *composing = if tag == "M" {
                        rest.into()
                    } else {
                        String::new()
                    };
                }
                _ => {}
            }
        }
    }

    #[test]
    fn typing_dkssud_gives_korean_through_the_c_interface() {
        let (dir, h) = new_handle("known-context");
        let (mut field, mut composing) = (String::new(), String::new());
        for ch in "dkssud".chars() {
            let out = call(h, 0, ch, Some(field.as_str()));
            assert!(out.starts_with('1'), "{out:?}");
            apply(&mut field, &mut composing, &out);
        }
        let out = call(h, 5, '\0', Some(field.as_str()));
        apply(&mut field, &mut composing, &out);
        assert_eq!(field.trim_end(), "안녕");
        assert_eq!(ts_paused(h), 0);
        unsafe { ts_free(h) };
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn null_context_retains_composition_across_keystrokes() {
        let (dir, h) = new_handle("null-compose");
        let (mut field, mut composing) = (String::new(), String::new());
        type_without_context(h, &mut field, &mut composing, "dkssudgktpdy");
        let out = call(h, 5, '\0', None);
        assert!(out.starts_with('1'), "{out:?}");
        apply(&mut field, &mut composing, &out);
        assert_eq!(field, "안녕하세요");
        unsafe { ts_free(h) };
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn losing_document_access_mid_composition_preserves_a_prefixed_word() {
        let (dir, h) = new_handle("context-lost-mid-word");
        let (mut field, mut composing) = (String::from("prefix "), String::new());
        let out = call(h, 0, 'd', Some(field.as_str()));
        apply(&mut field, &mut composing, &out);

        type_without_context(h, &mut field, &mut composing, "kssudgktpdy");
        assert_eq!(field, "prefix 안녕하세요");

        // Document access may become available again before the word is committed.
        let out = call(h, 5, '\0', Some(field.as_str()));
        assert!(out.starts_with('1'), "{out:?}");
        apply(&mut field, &mut composing, &out);
        assert_eq!(field, "prefix 안녕하세요");
        assert!(composing.is_empty());
        unsafe { ts_free(h) };
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn null_context_supports_delete_escape_enter_and_pause() {
        let (dir, h) = new_handle("null-commands");
        let (mut field, mut composing) = (String::new(), String::new());
        type_without_context(h, &mut field, &mut composing, "dkssud");
        assert_eq!(field, "안녕");

        let out = call(h, 2, '\0', None);
        assert!(out.starts_with('1'), "{out:?}");
        apply(&mut field, &mut composing, &out);
        assert_eq!(field, "안녀");

        let out = call(h, 3, '\0', None);
        assert!(out.starts_with('1'), "{out:?}");
        apply(&mut field, &mut composing, &out);
        assert_eq!(field, "dkssu");

        let out = call(h, 5, '\0', None);
        assert!(out.starts_with('1'), "{out:?}");
        apply(&mut field, &mut composing, &out);
        assert!(composing.is_empty());
        assert_eq!(field, "dkssu");

        type_without_context(h, &mut field, &mut composing, "dkssud");
        let out = result(ts_toggle_pause(h, std::ptr::null()));
        assert!(out.contains("\u{1}I안녕"), "{out:?}");
        apply(&mut field, &mut composing, &out);
        assert!(composing.is_empty());
        assert!(field.ends_with("안녕"));
        assert_eq!(ts_paused(h), 1);

        let out = result(ts_toggle_pause(h, std::ptr::null()));
        assert!(out.starts_with('1'), "{out:?}");
        assert_eq!(ts_paused(h), 0);
        unsafe { ts_free(h) };
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn known_empty_context_resets_a_nonempty_mark() {
        let (dir, h) = new_handle("empty-context");
        let (mut field, mut composing) = (String::new(), String::new());
        let first = call(h, 0, 'd', Some(""));
        assert!(first.starts_with('1'), "{first:?}");
        apply(&mut field, &mut composing, &first);
        assert!(!composing.is_empty());

        let after_reset = call(h, 0, 'k', Some(""));
        let (fresh_dir, fresh) = new_handle("empty-context-fresh");
        let fresh_key = call(fresh, 0, 'k', Some(""));
        assert_eq!(after_reset, fresh_key);

        unsafe { ts_free(h) };
        unsafe { ts_free(fresh) };
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(fresh_dir);
    }

    #[test]
    fn null_context_composition_does_not_leak_between_handles() {
        let (first_dir, first) = new_handle("isolated-first");
        let (second_dir, second) = new_handle("isolated-second");
        let (mut first_field, mut first_composing) = (String::new(), String::new());
        let (mut second_field, mut second_composing) = (String::new(), String::new());

        for (a, b) in "dkssud".chars().zip("gksrnr".chars()) {
            let out = call(first, 0, a, None);
            assert!(out.starts_with('1'), "{out:?}");
            apply(&mut first_field, &mut first_composing, &out);

            let out = call(second, 0, b, None);
            assert!(out.starts_with('1'), "{out:?}");
            apply(&mut second_field, &mut second_composing, &out);
        }

        assert_eq!(first_field, "안녕");
        assert_eq!(second_field, "한국");
        unsafe { ts_free(first) };
        unsafe { ts_free(second) };
        let _ = std::fs::remove_dir_all(first_dir);
        let _ = std::fs::remove_dir_all(second_dir);
    }

    #[test]
    fn a_null_handle_is_ignored() {
        assert_eq!(call(std::ptr::null_mut(), 0, 'a', None), "0");
        unsafe { ts_free(std::ptr::null_mut()) };
    }

    #[test]
    fn activating_picks_up_changed_settings() {
        let dir = std::env::temp_dir().join(format!("tacit-ffi-reload-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let settings = dir.join("settings.json");
        std::fs::write(&settings, br#"{"returnConfirmsOnly":true}"#).unwrap();
        let dir_c = CString::new(dir.to_string_lossy().as_bytes()).unwrap();
        let h = ts_new(dir_c.as_ptr());
        assert!(!h.is_null());
        let confirms = || {
            ts_activate(h, c"notes".as_ptr());
            let (mut field, mut composing) = (String::new(), String::new());
            for ch in "hello".chars() {
                let reply = call(h, 0, ch, Some(field.as_str()));
                apply(&mut field, &mut composing, &reply);
            }
            call(h, 5, '\0', Some(field.as_str())).starts_with('1')
        };
        assert!(confirms());
        std::fs::write(&settings, br#"{"returnConfirmsOnly":false}"#).unwrap();
        assert!(!confirms());
        unsafe { ts_free(h) };
        let _ = std::fs::remove_file(settings);
        let _ = std::fs::remove_file(dir.join("lexicon.json"));
        let _ = std::fs::remove_dir(dir);
    }
}

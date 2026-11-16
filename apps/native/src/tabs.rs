//! Which files are open as tabs, which one is showing, and which one is the
//! preview — the rules Zed's pane follows, kept apart from drawing so they
//! can be tested on their own.
//!
//! * A single click on a file opens it as the *preview* tab. There is at most
//!   one; opening another preview replaces it in place.
//! * Double-clicking the file or its tab, or doing something in it, keeps it:
//!   it stops being the preview and stays until closed.
//! * New tabs open just after the active one.
//! * Closing the active tab shows the tab used before it, else its left
//!   neighbour.

/// Tabs keyed by `FileChange::id`, which survives a refresh where an index
/// would not.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tabs {
    keys: Vec<String>,
    active: Option<usize>,
    preview: Option<String>,
    /// Previously active tabs, oldest first.
    history: Vec<String>,
}

impl Tabs {
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn active(&self) -> Option<&str> {
        self.active.map(|ix| self.keys[ix].as_str())
    }

    pub fn is_preview(&self, key: &str) -> bool {
        self.preview.as_deref() == Some(key)
    }

    pub fn position(&self, key: &str) -> Option<usize> {
        self.keys.iter().position(|k| k == key)
    }

    /// Opens `key` and makes it active. Returns whether a tab was created —
    /// the caller then has a file to load.
    pub fn open(&mut self, key: &str, preview: bool) -> bool {
        if let Some(ix) = self.position(key) {
            if !preview {
                self.keep(key);
            }
            self.activate(ix);
            return false;
        }
        let at = match (preview, self.preview.take()) {
            // The new preview takes the old one's place.
            (true, Some(old)) => {
                let ix = self.position(&old).expect("the preview is a tab");
                // The replaced preview is gone, not something to go back to.
                if self.active == Some(ix) {
                    self.active = None;
                }
                self.history.retain(|k| k != &old);
                self.remove(ix);
                ix
            }
            (_, old) => {
                self.preview = old;
                self.active.map_or(self.keys.len(), |ix| ix + 1)
            }
        };
        self.keys.insert(at, key.to_owned());
        if let Some(active) = self.active.as_mut()
            && *active >= at
        {
            *active += 1;
        }
        if preview {
            self.preview = Some(key.to_owned());
        }
        self.activate(at);
        true
    }

    /// Turns the preview tab `key` into a kept one; does nothing otherwise.
    pub fn keep(&mut self, key: &str) {
        if self.is_preview(key) {
            self.preview = None;
        }
    }

    pub fn activate(&mut self, ix: usize) {
        if ix >= self.keys.len() || self.active == Some(ix) {
            return;
        }
        if let Some(previous) = self.active() {
            let previous = previous.to_owned();
            self.history.retain(|k| k != &previous);
            self.history.push(previous);
        }
        self.active = Some(ix);
    }

    /// The next tab, wrapping round; `forward` false goes the other way.
    pub fn cycle(&mut self, forward: bool) {
        let (Some(ix), len) = (self.active, self.keys.len()) else {
            return;
        };
        self.activate(if forward { (ix + 1) % len } else { (ix + len - 1) % len });
    }

    pub fn close(&mut self, ix: usize) {
        if ix >= self.keys.len() {
            return;
        }
        let was_active = self.active == Some(ix);
        let key = self.remove(ix);
        if self.is_preview(&key) {
            self.preview = None;
        }
        self.history.retain(|k| k != &key);
        if !was_active {
            return;
        }
        self.active = None;
        if self.keys.is_empty() {
            return;
        }
        let next = self
            .history
            .pop()
            .and_then(|k| self.position(&k))
            .unwrap_or(ix.saturating_sub(1));
        self.active = Some(next);
    }

    pub fn close_active(&mut self) {
        if let Some(ix) = self.active {
            self.close(ix);
        }
    }

    pub fn close_others(&mut self) {
        let Some(key) = self.active().map(str::to_owned) else {
            return;
        };
        self.preview = self.preview.take().filter(|p| p == &key);
        self.keys = vec![key];
        self.history.clear();
        self.active = Some(0);
    }

    /// Points the tab for `old` at `new` instead, in the same place and with
    /// the same active and preview state — for a file whose entry changed
    /// under it, as staging turns `unstaged:a.rs` into `staged:a.rs`. When
    /// `new` already has a tab, `old`'s simply closes.
    pub fn replace(&mut self, old: &str, new: &str) {
        let Some(ix) = self.position(old) else {
            return;
        };
        if self.position(new).is_some() {
            let was_active = self.active == Some(ix);
            self.close(ix);
            if was_active
                && let Some(target) = self.position(new)
            {
                self.activate(target);
            }
            return;
        }
        self.keys[ix] = new.to_owned();
        if self.is_preview(old) {
            self.preview = Some(new.to_owned());
        }
        for key in &mut self.history {
            if key == old {
                *key = new.to_owned();
            }
        }
    }

    /// Closes the tabs whose key `exists` rejects — files that are no longer
    /// changed after a refresh.
    pub fn retain(&mut self, exists: impl Fn(&str) -> bool) {
        let mut ix = self.keys.len();
        while ix > 0 {
            ix -= 1;
            if !exists(&self.keys[ix]) {
                self.close(ix);
            }
        }
    }

    /// Takes the key out and keeps `active` pointing at the same tab.
    fn remove(&mut self, ix: usize) -> String {
        let key = self.keys.remove(ix);
        if let Some(active) = self.active.as_mut()
            && *active > ix
        {
            *active -= 1;
        }
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(tabs: &Tabs) -> Vec<&str> {
        tabs.keys().iter().map(String::as_str).collect()
    }

    #[test]
    fn a_click_opens_a_preview_and_the_next_click_replaces_it() {
        let mut tabs = Tabs::default();
        assert!(tabs.open("a", true));
        assert!(tabs.is_preview("a"));
        assert!(tabs.open("b", true));
        assert_eq!(keys(&tabs), ["b"]);
        assert_eq!(tabs.active(), Some("b"));
    }

    #[test]
    fn a_kept_tab_stays_when_the_next_preview_opens() {
        let mut tabs = Tabs::default();
        tabs.open("a", true);
        tabs.keep("a");
        tabs.open("b", true);
        assert_eq!(keys(&tabs), ["a", "b"]);
        assert!(!tabs.is_preview("a"));
        assert!(tabs.is_preview("b"));
    }

    #[test]
    fn a_double_click_on_an_open_preview_keeps_it() {
        let mut tabs = Tabs::default();
        tabs.open("a", true);
        assert!(!tabs.open("a", false));
        assert!(!tabs.is_preview("a"));
    }

    #[test]
    fn the_preview_is_replaced_where_it_stands() {
        let mut tabs = Tabs::default();
        tabs.open("a", false);
        tabs.open("p", true);
        tabs.open("c", false);
        tabs.activate(0);
        tabs.open("q", true);
        assert_eq!(keys(&tabs), ["a", "q", "c"]);
        assert_eq!(tabs.active(), Some("q"));
    }

    #[test]
    fn new_tabs_open_after_the_active_one() {
        let mut tabs = Tabs::default();
        tabs.open("a", false);
        tabs.open("b", false);
        tabs.activate(0);
        tabs.open("c", false);
        assert_eq!(keys(&tabs), ["a", "c", "b"]);
    }

    #[test]
    fn opening_an_open_file_just_switches_to_it() {
        let mut tabs = Tabs::default();
        tabs.open("a", false);
        tabs.open("b", false);
        assert!(!tabs.open("a", true));
        assert_eq!(keys(&tabs), ["a", "b"]);
        assert_eq!(tabs.active(), Some("a"));
        assert!(!tabs.is_preview("a"));
    }

    #[test]
    fn closing_the_active_tab_goes_back_to_the_one_used_before() {
        let mut tabs = Tabs::default();
        tabs.open("a", false);
        tabs.open("b", false);
        tabs.open("c", false);
        tabs.activate(0);
        tabs.activate(2);
        tabs.close_active();
        assert_eq!(tabs.active(), Some("a"));
    }

    #[test]
    fn closing_without_history_goes_left() {
        let mut tabs = Tabs::default();
        tabs.open("a", false);
        tabs.open("b", false);
        tabs.history.clear();
        tabs.close_active();
        assert_eq!(tabs.active(), Some("a"));
    }

    #[test]
    fn closing_another_tab_keeps_the_active_one() {
        let mut tabs = Tabs::default();
        tabs.open("a", false);
        tabs.open("b", false);
        tabs.open("c", false);
        tabs.close(0);
        assert_eq!(tabs.active(), Some("c"));
        assert_eq!(tabs.active_index(), Some(1));
    }

    #[test]
    fn closing_the_last_tab_leaves_nothing_active() {
        let mut tabs = Tabs::default();
        tabs.open("a", true);
        tabs.close_active();
        assert_eq!(tabs.active(), None);
        assert!(!tabs.is_preview("a"));
    }

    #[test]
    fn cycling_wraps_round() {
        let mut tabs = Tabs::default();
        tabs.open("a", false);
        tabs.open("b", false);
        tabs.cycle(true);
        assert_eq!(tabs.active(), Some("a"));
        tabs.cycle(false);
        assert_eq!(tabs.active(), Some("b"));
    }

    #[test]
    fn close_others_keeps_only_the_active_tab() {
        let mut tabs = Tabs::default();
        tabs.open("a", false);
        tabs.open("b", true);
        tabs.activate(0);
        tabs.close_others();
        assert_eq!(keys(&tabs), ["a"]);
        assert!(!tabs.is_preview("b"));
    }

    #[test]
    fn a_staged_file_keeps_its_tab() {
        let mut tabs = Tabs::default();
        tabs.open("unstaged:a", false);
        tabs.open("unstaged:b", true);
        tabs.replace("unstaged:b", "staged:b");
        assert_eq!(keys(&tabs), ["unstaged:a", "staged:b"]);
        assert_eq!(tabs.active(), Some("staged:b"));
        assert!(tabs.is_preview("staged:b"));
    }

    #[test]
    fn replacing_onto_an_open_tab_closes_the_old_one_and_shows_the_new() {
        let mut tabs = Tabs::default();
        tabs.open("staged:a", false);
        tabs.open("unstaged:a", false);
        tabs.replace("unstaged:a", "staged:a");
        assert_eq!(keys(&tabs), ["staged:a"]);
        assert_eq!(tabs.active(), Some("staged:a"));
    }

    #[test]
    fn a_refresh_closes_tabs_for_files_no_longer_changed() {
        let mut tabs = Tabs::default();
        tabs.open("a", false);
        tabs.open("b", false);
        tabs.open("c", false);
        tabs.retain(|k| k != "c" && k != "a");
        assert_eq!(keys(&tabs), ["b"]);
        assert_eq!(tabs.active(), Some("b"));
    }
}

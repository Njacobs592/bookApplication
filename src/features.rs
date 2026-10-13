//! UI-independent writing tools and persistent workspace state.

use chrono::Local;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn unique_id() -> String {
    format!(
        "{:x}-{:x}-{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Preferences {
    pub font: String,
    pub font_size: u32,
    pub line_spacing: u32,
    pub text_width: i32,
    pub theme: String,
    pub include_lore: bool,
    pub daily_goal: usize,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            font: "Serif".into(),
            font_size: 16,
            line_spacing: 6,
            text_width: 800,
            theme: "system".into(),
            include_lore: false,
            daily_goal: 500,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceState {
    pub storage: PathBuf,
    pub project: PathBuf,
    pub chapter: Option<PathBuf>,
    pub lore: Option<PathBuf>,
    pub tab: u32,
    pub width: i32,
    pub height: i32,
    pub chapter_sidebar: i32,
    pub lore_sidebar: i32,
    pub cursors: BTreeMap<PathBuf, i32>,
    pub scrolls: BTreeMap<PathBuf, f64>,
    pub preferences: Preferences,
    pub progress: Progress,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            storage: PathBuf::new(),
            project: PathBuf::new(),
            chapter: None,
            lore: None,
            tab: 0,
            width: 1200,
            height: 800,
            chapter_sidebar: 240,
            lore_sidebar: 240,
            cursors: BTreeMap::new(),
            scrolls: BTreeMap::new(),
            preferences: Preferences::default(),
            progress: Progress::default(),
        }
    }
}

/// `config` is a directory, not a filename. Invalid existing JSON is an error.
pub fn load_workspace(config: &Path, initial_root: &Path) -> io::Result<WorkspaceState> {
    let mut state: WorkspaceState = read_json(&config.join("workspace.json"))?.unwrap_or_default();
    if state.storage.as_os_str().is_empty() {
        state.storage = initial_root.to_path_buf();
    }
    if state.project.as_os_str().is_empty() {
        state.project = state.storage.clone();
    }
    state.progress.rollover(&today());
    validate_preferences(&state.preferences)?;
    Ok(state)
}

pub fn save_workspace(config: &Path, state: &WorkspaceState) -> io::Result<()> {
    validate_preferences(&state.preferences)?;
    write_json(&config.join("workspace.json"), state)
}

fn validate_preferences(preferences: &Preferences) -> io::Result<()> {
    if preferences.font.trim().is_empty()
        || !(8..=72).contains(&preferences.font_size)
        || preferences.line_spacing > 32
        || !(320..=1600).contains(&preferences.text_width)
        || !matches!(preferences.theme.as_str(), "system" | "light" | "dark")
    {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid editor preferences: use a nonempty font, size 8–72, spacing 0–32, width 320–1600, and system/light/dark theme"));
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    match fs::read(path) {
        Ok(data) => serde_json::from_slice(&data)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn write_json<T: Serialize + DeserializeOwned>(path: &Path, value: &T) -> io::Result<()> {
    // Never silently replace an unreadable or malformed existing configuration.
    read_json::<T>(path)?;
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "configuration file must not be a symlink",
        ));
    }
    let data = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    // JSON represents nonfinite floats as null, which cannot restore an f64.
    serde_json::from_slice::<T>(&data)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "configuration has no parent")
    })?;
    fs::create_dir_all(parent)?;
    let pending = parent.join(format!(".bookapp-pending-{}.json", unique_id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        file.write_all(&data)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&pending, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

const HISTORY_STATES: usize = 100;
const HISTORY_BYTES: usize = 16 * 1024 * 1024;

/// Keep one history per document; call `record` from a debounced UI edit handler.
#[derive(Clone, Debug)]
pub struct UndoHistory {
    states: Vec<String>,
    position: usize,
    bytes: usize,
}

impl UndoHistory {
    pub fn new(text: &str) -> Self {
        Self {
            states: vec![text.to_string()],
            position: 0,
            bytes: text.len(),
        }
    }

    pub fn current(&self) -> &str {
        &self.states[self.position]
    }

    pub fn record(&mut self, text: &str) {
        if self.current() == text {
            return;
        }
        self.states.truncate(self.position + 1);
        self.bytes = self.states.iter().map(String::len).sum();
        self.states.push(text.to_string());
        self.bytes = self.bytes.saturating_add(text.len());
        while self.states.len() > 1
            && (self.states.len() > HISTORY_STATES || self.bytes > HISTORY_BYTES)
        {
            self.bytes -= self.states.remove(0).len();
        }
        self.position = self.states.len() - 1;
    }

    pub fn undo(&mut self) -> Option<String> {
        if self.position == 0 {
            return None;
        }
        self.position -= 1;
        Some(self.current().to_string())
    }

    pub fn redo(&mut self) -> Option<String> {
        if self.position + 1 == self.states.len() {
            return None;
        }
        self.position += 1;
        Some(self.current().to_string())
    }

    pub fn reset(&mut self, text: &str) {
        *self = Self::new(text);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextMatch {
    /// Unicode scalar offsets, matching GtkTextIter offsets (not byte offsets).
    pub start: i32,
    pub end: i32,
    /// One-based display line.
    pub line: usize,
    pub excerpt: String,
}

fn folded_char(c: char, case_sensitive: bool) -> Vec<char> {
    if case_sensitive {
        return vec![c];
    }
    match c {
        'ß' | 'ẞ' => vec!['s', 's'],
        'ı' => vec!['ı'],
        // Upper/lower expansion gives caseless equivalence for ligatures,
        // Greek variants and Armenian, without invalidating source offsets.
        _ => c.to_uppercase().flat_map(char::to_lowercase).collect(),
    }
}

fn combining_mark(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036f | 0x0483..=0x0489 | 0x0591..=0x05bd |
        0x05bf | 0x05c1..=0x05c2 | 0x05c4..=0x05c5 | 0x0610..=0x061a |
        0x064b..=0x065f | 0x0670 | 0x06d6..=0x06ed | 0x1ab0..=0x1aff |
        0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || combining_mark(c)
}

fn match_ranges(content: &str, query: &str, options: SearchOptions) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let original: Vec<char> = content.chars().collect();
    let needle: Vec<char> = query
        .chars()
        .flat_map(|c| folded_char(c, options.case_sensitive))
        .collect();
    let mut folded = Vec::new();
    let mut boundaries = vec![Some(0)];
    for (index, c) in original.iter().enumerate() {
        let expansion = folded_char(*c, options.case_sensitive);
        for (part, character) in expansion.iter().enumerate() {
            folded.push(*character);
            boundaries.push(if part + 1 == expansion.len() {
                Some(index + 1)
            } else {
                None
            });
        }
    }
    // Prefix matching keeps repeated-character manuscripts and long queries
    // linear rather than checking the complete query at every character.
    let mut prefix = vec![0; needle.len()];
    let mut matched = 0;
    for index in 1..needle.len() {
        while matched > 0 && needle[index] != needle[matched] {
            matched = prefix[matched - 1];
        }
        if needle[index] == needle[matched] {
            matched += 1;
        }
        prefix[index] = matched;
    }
    let mut matches = Vec::new();
    matched = 0;
    for (index, character) in folded.iter().enumerate() {
        while matched > 0 && *character != needle[matched] {
            matched = prefix[matched - 1];
        }
        if *character == needle[matched] {
            matched += 1;
        }
        if matched == needle.len() {
            let end = index + 1;
            let start = end - needle.len();
            if let (Some(start_char), Some(end_char)) = (boundaries[start], boundaries[end]) {
                let whole = !options.whole_word
                    || ((start_char == 0 || !word_char(original[start_char - 1]))
                        && (end_char == original.len() || !word_char(original[end_char])));
                if whole {
                    matches.push((start_char, end_char));
                    matched = 0;
                    continue;
                }
            }
            matched = prefix[matched - 1];
        }
    }
    matches
}

pub fn find_matches(content: &str, query: &str, options: SearchOptions) -> Vec<TextMatch> {
    let ranges = match_ranges(content, query, options);
    let mut line_starts = vec![0];
    let characters: Vec<char> = content.chars().collect();
    for (index, c) in characters.iter().enumerate() {
        if *c == '\n' {
            line_starts.push(index + 1);
        }
    }
    ranges
        .into_iter()
        .map(|(start, end)| {
            let line_index = line_starts.partition_point(|position| *position <= start) - 1;
            let line_end = line_starts
                .get(line_index + 1)
                .map(|position| position - 1)
                .unwrap_or(characters.len());
            let excerpt: String = characters[line_starts[line_index]..line_end]
                .iter()
                .take(160)
                .collect();
            TextMatch {
                start: start.min(i32::MAX as usize) as i32,
                end: end.min(i32::MAX as usize) as i32,
                line: line_index + 1,
                excerpt: excerpt.trim_end_matches('\r').to_string(),
            }
        })
        .collect()
}

pub fn replace_matches(
    content: &str,
    query: &str,
    replacement: &str,
    options: SearchOptions,
) -> (String, usize) {
    let ranges = match_ranges(content, query, options);
    let boundaries: Vec<usize> = content
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(content.len()))
        .collect();
    let mut result = String::with_capacity(content.len());
    let mut previous = 0;
    for &(start, end) in &ranges {
        result.push_str(&content[boundaries[previous]..boundaries[start]]);
        result.push_str(replacement);
        previous = end;
    }
    result.push_str(&content[boundaries[previous]..]);
    (result, ranges.len())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    /// Zero-based line number for GtkTextBuffer::iter_at_line.
    pub line: i32,
    pub level: usize,
    pub title: String,
}

pub fn headings(content: &str) -> Vec<Heading> {
    let mut result = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for (line, text) in content.lines().enumerate() {
        let trimmed = text.trim_start_matches(' ');
        if text.len() - trimmed.len() > 3 {
            continue;
        }
        let first = trimmed.chars().next().unwrap_or('\0');
        if first == '`' || first == '~' {
            let count = trimmed.chars().take_while(|c| *c == first).count();
            if count >= 3 {
                match fence {
                    None => fence = Some((first, count)),
                    Some((kind, length))
                        if kind == first
                            && count >= length
                            && trimmed[count..].trim().is_empty() =>
                    {
                        fence = None;
                    }
                    _ => {}
                }
                continue;
            }
        }
        if fence.is_some() {
            continue;
        }
        let level = trimmed.bytes().take_while(|c| *c == b'#').count();
        if !(1..=6).contains(&level) {
            continue;
        }
        let rest = &trimmed[level..];
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let mut title = rest.trim();
        let without_closing = title.trim_end_matches('#');
        if without_closing.len() != title.len()
            && (without_closing.is_empty() || without_closing.ends_with(char::is_whitespace))
        {
            title = without_closing.trim_end();
        }
        result.push(Heading {
            line: line.min(i32::MAX as usize) as i32,
            level,
            title: title.into(),
        });
    }
    result
}

/// Count Unicode alphanumeric runs, retaining apostrophes inside a word.
pub fn word_count(content: &str) -> usize {
    let mut count = 0;
    let mut in_word = false;
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_alphanumeric() {
            if !in_word {
                count += 1;
            }
            in_word = true;
        } else if !(in_word
            && (combining_mark(c)
                || ((c == '\'' || c == '’')
                    && chars.peek().is_some_and(|next| next.is_alphanumeric()))))
        {
            in_word = false;
        }
    }
    count
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Metadata {
    pub id: String,
    pub summary: String,
    pub pov: String,
    pub timeline: String,
    pub status: String,
    pub tags: Vec<String>,
    /// Stable metadata IDs, not paths; deleted IDs remain as unresolved links.
    pub links: Vec<String>,
    pub target: usize,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            id: unique_id(),
            summary: String::new(),
            pov: String::new(),
            timeline: String::new(),
            status: "Draft".into(),
            tags: Vec::new(),
            links: Vec::new(),
            target: 0,
        }
    }
}

fn metadata_path(path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "document has no filename"))?;
    let mut filename = name.to_os_string();
    filename.push(".json");
    Ok(path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".metadata")
        .join(filename))
}

/// Missing metadata gets a fresh ID; save it immediately to establish identity.
pub fn load_metadata(path: &Path) -> io::Result<Metadata> {
    let mut metadata: Metadata = read_json(&metadata_path(path)?)?.unwrap_or_default();
    if metadata.id.is_empty() {
        metadata.id = unique_id();
    }
    Ok(metadata)
}

pub fn save_metadata(path: &Path, metadata: &Metadata) -> io::Result<()> {
    if metadata.id.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "metadata identity cannot be empty",
        ));
    }
    write_json(&metadata_path(path)?, metadata)
}

pub fn template(category: &str) -> &'static str {
    match category.to_lowercase().as_str() {
        "character" | "characters" => {
            "# Character\n\n## Role\n\n## Appearance\n\n## Personality\n\n## Motivation\n\n## Relationships\n\n## History\n"
        }
        "environment" | "environments" | "location" | "locations" => {
            "# Environment\n\n## Description\n\n## Atmosphere\n\n## Geography\n\n## Inhabitants\n\n## History\n\n## Story significance\n"
        }
        _ => "# Lore\n\n## Summary\n\n## Details\n\n## Story connections\n",
    }
}

fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentProgress {
    pub is_lore: bool,
    pub current: usize,
    pub session_baseline: usize,
    pub daily_baseline: usize,
    pub session_gross: usize,
    pub daily_gross: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub date: String,
    pub documents: BTreeMap<String, DocumentProgress>,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            date: today(),
            documents: BTreeMap::new(),
        }
    }
}

impl Progress {
    /// Record an edit, not a document load. Track lore even while excluded so a
    /// goal preference toggle can recompute totals without losing any history.
    pub fn record(
        &mut self,
        id: &str,
        before: usize,
        after: usize,
        is_lore: bool,
        _include_lore: bool,
    ) {
        self.rollover(&today());
        let document = self
            .documents
            .entry(id.to_string())
            .or_insert_with(|| DocumentProgress {
                is_lore,
                current: before,
                session_baseline: before,
                daily_baseline: before,
                ..DocumentProgress::default()
            });
        document.is_lore = is_lore;
        document.current = after;
        let added = after.saturating_sub(before);
        document.session_gross = document.session_gross.saturating_add(added);
        document.daily_gross = document.daily_gross.saturating_add(added);
    }

    /// Net growth from the session baseline; deletion/undo may be negative.
    pub fn net(&self, include_lore: bool) -> i64 {
        self.net_for(include_lore, false)
    }

    pub fn daily_net(&self, include_lore: bool) -> i64 {
        if self.date != today() {
            0
        } else {
            self.net_for(include_lore, true)
        }
    }

    fn net_for(&self, include_lore: bool, daily: bool) -> i64 {
        let total: i128 = self
            .documents
            .values()
            .filter(|document| include_lore || !document.is_lore)
            .map(|document| {
                document.current as i128
                    - if daily {
                        document.daily_baseline
                    } else {
                        document.session_baseline
                    } as i128
            })
            .sum();
        total.clamp(i64::MIN as i128, i64::MAX as i128) as i64
    }

    /// Gross additions include retyping and redo. Use net growth for goals.
    pub fn gross(&self, include_lore: bool) -> usize {
        self.gross_for(include_lore, false)
    }

    pub fn daily_gross(&self, include_lore: bool) -> usize {
        if self.date != today() {
            0
        } else {
            self.gross_for(include_lore, true)
        }
    }

    fn gross_for(&self, include_lore: bool, daily: bool) -> usize {
        self.documents
            .values()
            .filter(|document| include_lore || !document.is_lore)
            .fold(0usize, |total, document| {
                total.saturating_add(if daily {
                    document.daily_gross
                } else {
                    document.session_gross
                })
            })
    }

    /// Call at application startup or when the writer explicitly starts anew.
    /// Daily growth persists across sessions.
    pub fn start_session(&mut self) {
        self.rollover(&today());
        for document in self.documents.values_mut() {
            document.session_baseline = document.current;
            document.session_gross = 0;
        }
    }

    /// Pass a local ISO date. An unchanged date preserves persisted daily goals.
    pub fn rollover(&mut self, date: &str) {
        if self.date == date {
            return;
        }
        self.date = date.to_string();
        for document in self.documents.values_mut() {
            document.daily_baseline = document.current;
            document.daily_gross = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = PathBuf::from(format!(".bookapp-features-test-{}", unique_id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn undo_redo_branching_and_duplicate_edits() {
        let mut history = UndoHistory::new("one");
        history.record("two");
        history.record("two");
        history.record("three");
        assert_eq!(history.undo().as_deref(), Some("two"));
        assert_eq!(history.undo().as_deref(), Some("one"));
        assert_eq!(history.undo(), None);
        assert_eq!(history.redo().as_deref(), Some("two"));
        history.record("branch");
        assert_eq!(history.redo(), None);
        assert_eq!(history.undo().as_deref(), Some("two"));
        history.reset("new document");
        assert_eq!(history.current(), "new document");
        assert_eq!(history.undo(), None);
    }

    #[test]
    fn invalid_editor_preferences_are_reported_without_overwriting_configuration() {
        let dir = TestDirectory::new();
        let mut state = WorkspaceState::default();
        state.preferences.font_size = u32::MAX;
        let contents = serde_json::to_vec(&state).unwrap();
        let path = dir.0.join("workspace.json");
        fs::write(&path, &contents).unwrap();
        assert!(load_workspace(&dir.0, Path::new("book")).is_err());
        assert!(save_workspace(&dir.0, &state).is_err());
        assert_eq!(fs::read(path).unwrap(), contents);
    }

    #[test]
    fn history_bounds_states_and_bytes() {
        let mut history = UndoHistory::new("");
        for index in 0..300 {
            history.record(&index.to_string());
        }
        assert_eq!(history.states.len(), HISTORY_STATES);
        assert_eq!(history.current(), "299");
        let large = "a".repeat(HISTORY_BYTES / 3);
        for suffix in ["1", "2", "3", "4"] {
            history.record(&format!("{large}{suffix}"));
        }
        assert!(history.bytes <= HISTORY_BYTES);
        assert!(history.states.len() >= 2);
        history.record(&"b".repeat(HISTORY_BYTES + 1));
        assert_eq!(history.states.len(), 1);
        assert_eq!(history.undo(), None);
    }

    #[test]
    fn unicode_search_uses_character_offsets_and_expansion_boundaries() {
        let content = "猫 Élan\nStraße İx Σς ﬃ";
        let options = SearchOptions::default();
        assert_eq!(find_matches(content, "élan", options)[0].start, 2);
        let found = find_matches(content, "STRASSE", options);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].start, found[0].end, found[0].line), (7, 13, 2));
        assert_eq!(find_matches(content, "i\u{307}x", options)[0].start, 14);
        assert!(find_matches("İ", "i", options).is_empty());
        assert!(find_matches("ß", "s", options).is_empty());
        assert_eq!(find_matches("Σς", "σσ", options).len(), 1);
        assert_eq!(find_matches("ﬃ", "FFI", options).len(), 1);
        assert_eq!(find_matches("ϐ ϑ µ և", "β", options).len(), 1);
        assert_eq!(find_matches("ϐ ϑ µ և", "θ", options).len(), 1);
        assert_eq!(find_matches("ϐ ϑ µ և", "μ", options).len(), 1);
        assert_eq!(find_matches("ϐ ϑ µ և", "ԵՒ", options).len(), 1);
        assert!(find_matches("ı", "i", options).is_empty());
        let (replaced, count) = replace_matches(content, "strasse", "道", options);
        assert_eq!(count, 1);
        assert_eq!(replaced, "猫 Élan\n道 İx Σς ﬃ");
    }

    #[test]
    fn literal_case_whole_word_and_nonoverlapping_replacement() {
        let options = SearchOptions {
            whole_word: true,
            ..SearchOptions::default()
        };
        assert_eq!(
            find_matches("cat scatter cat_ cat猫 CAT cat.", "cat", options).len(),
            3
        );
        assert!(find_matches("e\u{301}", "e", options).is_empty());
        assert_eq!(
            replace_matches("aaaaa", "aa", "b", SearchOptions::default()),
            ("bba".into(), 2)
        );
        assert_eq!(
            replace_matches("a.b aXb", "a.b", "$1", SearchOptions::default()),
            ("$1 aXb".into(), 1)
        );
        assert!(find_matches("anything", "", options).is_empty());
        assert!(find_matches(
            "CAT",
            "cat",
            SearchOptions {
                case_sensitive: true,
                whole_word: false
            }
        )
        .is_empty());
    }

    #[test]
    fn headings_ignore_code_and_obey_markdown_levels() {
        let outline = headings(
            "# Top #\ntext\n  ## Child\n###No\n####### Too many\n```\n# code\n```\n###### Six\n#\n",
        );
        assert_eq!(
            outline,
            vec![
                Heading {
                    line: 0,
                    level: 1,
                    title: "Top".into()
                },
                Heading {
                    line: 2,
                    level: 2,
                    title: "Child".into()
                },
                Heading {
                    line: 8,
                    level: 6,
                    title: "Six".into()
                },
                Heading {
                    line: 9,
                    level: 1,
                    title: "".into()
                }
            ]
        );
        assert_eq!(
            word_count("  hello,world! can't l’esprit e\u{301}lan 猫 42 -- "),
            7
        );
        assert_eq!(word_count("# --- 😊"), 0);
    }

    #[test]
    fn metadata_identity_and_links_survive_save_and_sidecar_move() {
        let directory = TestDirectory::new();
        let document = directory.0.join("old.txt");
        let mut metadata = load_metadata(&document).unwrap();
        metadata.summary = "Scene summary".into();
        metadata.links = vec!["missing-id".into()];
        metadata.tags = vec!["turning point".into()];
        save_metadata(&document, &metadata).unwrap();
        assert_eq!(load_metadata(&document).unwrap(), metadata);
        let renamed = directory.0.join("new.txt");
        fs::rename(
            metadata_path(&document).unwrap(),
            metadata_path(&renamed).unwrap(),
        )
        .unwrap();
        assert_eq!(load_metadata(&renamed).unwrap(), metadata);
        assert_ne!(Metadata::default().id, Metadata::default().id);
    }

    #[test]
    fn metadata_and_workspace_malformed_files_are_not_overwritten() {
        let directory = TestDirectory::new();
        let document = directory.0.join("draft.txt");
        let metadata = Metadata::default();
        save_metadata(&document, &metadata).unwrap();
        let sidecar = metadata_path(&document).unwrap();
        fs::write(&sidecar, b"{broken").unwrap();
        assert_eq!(
            load_metadata(&document).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(save_metadata(&document, &metadata).is_err());
        assert_eq!(fs::read_to_string(sidecar).unwrap(), "{broken");
        let config = directory.0.join("config");
        let state = load_workspace(&config, &directory.0).unwrap();
        save_workspace(&config, &state).unwrap();
        fs::write(config.join("workspace.json"), "[invalid").unwrap();
        assert!(load_workspace(&config, &directory.0).is_err());
        assert!(save_workspace(&config, &state).is_err());
        assert_eq!(
            fs::read_to_string(config.join("workspace.json")).unwrap(),
            "[invalid"
        );
    }

    #[test]
    fn workspace_preferences_selection_and_positions_roundtrip() {
        let directory = TestDirectory::new();
        let mut state = load_workspace(&directory.0, Path::new("book")).unwrap();
        assert_eq!(state.storage, Path::new("book"));
        assert_eq!(state.project, Path::new("book"));
        state.preferences.font = "Example Serif".into();
        state.preferences.theme = "dark".into();
        state.preferences.daily_goal = 1000;
        state.chapter = Some(PathBuf::from("book/01_猫.txt"));
        state.cursors.insert(state.chapter.clone().unwrap(), 42);
        state.scrolls.insert(state.chapter.clone().unwrap(), 12.25);
        save_workspace(&directory.0, &state).unwrap();
        let loaded = load_workspace(&directory.0, Path::new("elsewhere")).unwrap();
        assert_eq!(loaded.preferences, state.preferences);
        assert_eq!(loaded.chapter, state.chapter);
        assert_eq!(loaded.cursors, state.cursors);
        assert_eq!(loaded.scrolls, state.scrolls);
        assert_eq!(loaded.storage, state.storage);
        state
            .scrolls
            .insert(PathBuf::from("invalid"), f64::INFINITY);
        assert!(save_workspace(&directory.0, &state).is_err());
        assert_eq!(
            load_workspace(&directory.0, Path::new("elsewhere"))
                .unwrap()
                .scrolls,
            loaded.scrolls
        );
    }

    #[test]
    fn net_growth_does_not_reward_delete_retype_or_undo() {
        let mut progress = Progress::default();
        progress.record("chapter", 100, 80, false, false);
        assert_eq!(progress.net(false), -20);
        progress.record("chapter", 80, 100, false, false);
        assert_eq!(progress.net(false), 0);
        assert_eq!(progress.gross(false), 20);
        progress.record("chapter", 100, 120, false, false);
        progress.record("chapter", 120, 100, false, false);
        assert_eq!(progress.daily_net(false), 0);
        progress.record("chapter", 100, 120, false, false);
        assert_eq!(progress.net(false), 20);
        assert_eq!(progress.gross(false), 60);
    }

    #[test]
    fn progress_lore_is_optional_and_sessions_preserve_daily_growth() {
        let mut progress = Progress::default();
        progress.record("chapter", 0, 10, false, false);
        progress.record("lore", 0, 20, true, false);
        assert_eq!(progress.net(false), 10);
        assert_eq!(progress.net(true), 30);
        assert_eq!(progress.gross(false), 10);
        assert_eq!(progress.gross(true), 30);
        progress.start_session();
        assert_eq!(progress.net(true), 0);
        assert_eq!(progress.gross(true), 0);
        assert_eq!(progress.daily_net(true), 30);
        progress.record("chapter", 10, 15, false, false);
        assert_eq!(progress.net(false), 5);
        assert_eq!(progress.daily_net(false), 15);
    }

    #[test]
    fn progress_daily_rollover_and_restart_persist_baselines() {
        let directory = TestDirectory::new();
        let mut state = load_workspace(&directory.0, Path::new("book")).unwrap();
        state.progress.record("chapter", 100, 110, false, false);
        save_workspace(&directory.0, &state).unwrap();
        let mut restarted = load_workspace(&directory.0, Path::new("book")).unwrap();
        assert_eq!(restarted.progress.daily_net(false), 10);
        restarted.progress.start_session();
        assert_eq!(restarted.progress.daily_net(false), 10);
        assert_eq!(restarted.progress.net(false), 0);
        restarted.progress.date = "1999-12-31".into();
        save_workspace(&directory.0, &restarted).unwrap();
        let mut next_day = load_workspace(&directory.0, Path::new("book")).unwrap();
        assert_eq!(next_day.progress.daily_net(false), 0);
        assert_eq!(next_day.progress.daily_gross(false), 0);
        next_day.progress.record("chapter", 110, 115, false, false);
        assert_eq!(next_day.progress.daily_net(false), 5);
        let date = next_day.progress.date.clone();
        next_day.progress.rollover(&date);
        assert_eq!(next_day.progress.daily_net(false), 5);
    }
}

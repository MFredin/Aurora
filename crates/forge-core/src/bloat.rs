use serde::Deserialize;

static EMBEDDED: &str = include_str!("../data/bloat.json");

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Safe,
    Caution,
    Keep,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Appx,
    Startup,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    #[serde(rename = "match")]
    pub patterns: Vec<String>,
    pub verdict: Verdict,
    pub reason: String,
}

#[derive(Deserialize, Debug)]
pub struct BloatDb {
    pub version: u32,
    pub curated: bool,
    pub entries: Vec<Entry>,
}

impl BloatDb {
    pub fn embedded() -> Self {
        serde_json::from_str(EMBEDDED).expect("embedded bloat.json must be valid")
    }

    /// First entry of `kind` whose pattern appears in `haystack` (case-insensitive).
    pub fn lookup(&self, kind: Kind, haystack: &str) -> Option<&Entry> {
        let h = haystack.to_lowercase();
        self.entries
            .iter()
            .find(|e| e.kind == kind && e.patterns.iter().any(|p| h.contains(p.as_str())))
    }
}

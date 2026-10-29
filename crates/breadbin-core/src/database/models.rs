// models - Data types for the CSDb database.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: u32,
    pub name: String,
    pub short: Option<String>,
    pub base_country: Option<String>,
    pub founded_month: Option<u32>,
    pub founded_year: Option<u32>,
    pub website: Option<String>,
    pub rating: f32,
    pub group_types: Vec<String>,
    pub slogans: Vec<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scener {
    pub id: u32,
    pub handles: Vec<ScenerHandle>,
    pub country: Option<String>,
    pub trivia: Option<String>,
    pub handle_story: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenerHandle {
    pub id: Option<u64>,
    pub scener_id: u32,
    pub handle: String,
    pub aka: Option<String>,
    pub currently_used: bool,
    pub handle_story: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: u32,
    pub name: String,
    pub event_type: Option<String>,
    pub aka: Vec<String>,
    pub start_day: Option<u32>,
    pub start_month: Option<u32>,
    pub start_year: Option<u32>,
    pub end_day: Option<u32>,
    pub end_month: Option<u32>,
    pub end_year: Option<u32>,
    pub address: Option<String>,
    pub zip: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub website: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Release {
    pub id: u32,
    pub name: String,
    pub release_type_id: u32,
    pub release_type: String,
    pub group_id: Option<u32>,
    pub group: Option<Box<Group>>,
    pub rating: f32,
    pub release_day: Option<u32>,
    pub release_month: Option<u32>,
    pub release_year: Option<u32>,
    pub event_id: Option<u32>,
    pub event: Option<Box<Event>>,
    pub compo: Option<String>,
    pub place: u32,
    pub screenshot_url: Option<String>,
    pub screenshot_urls: Vec<String>,
    pub download_count: u32,
    pub website: Option<String>,
    pub credits: Vec<ReleaseCredit>,
    pub downloads: Vec<DownloadLink>,
    pub sids: Vec<SidFile>,
    pub ia_mappings: Vec<IAMapping>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseCredit {
    pub id: Option<u64>,
    pub release_id: u32,
    pub scener_id: Option<u32>,
    pub handle: String,
    pub credit_type: String,
    pub position: u32,
    pub functions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadLink {
    pub id: Option<u64>,
    pub release_id: u32,
    pub url: String,
    pub counter_url: Option<String>,
    pub downloads: u32,
    pub status: String,
    pub is_internal: bool,
    pub file_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidFile {
    pub id: Option<u64>,
    pub release_id: u32,
    pub sid_id: Option<u32>,
    pub hvsc_path: Option<String>,
    pub name: String,
    pub author: Option<String>,
    pub released: Option<String>,
    pub load_addr: Option<u32>,
    pub init_addr: Option<u32>,
    pub play_addr: Option<u32>,
    pub num_songs: u32,
    pub default_song: u32,
    pub sid_model: Option<String>,
    pub clock_speed: Option<String>,
    pub data_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IAMapping {
    pub id: Option<u64>,
    pub release_id: u32,
    pub ia_identifier: String,
    pub ia_title: Option<String>,
    pub ia_collection: String,
    pub match_confidence: f32,
    pub last_checked: Option<String>,
}

// Search types

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSort {
    Relevance,
    Rating,
    Date,
    Name,
    Downloads,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReleaseTypeFilter {
    All,
    Demo,
    Music,
    Game,
    Crack,
    Intro,
    Graphics,
    Diskmag,
    Tool,
    Custom(u32),
}

impl ReleaseTypeFilter {
    pub fn id(&self) -> u32 {
        match self {
            ReleaseTypeFilter::All => 0,
            ReleaseTypeFilter::Demo => 1,
            ReleaseTypeFilter::Music => 7,
            ReleaseTypeFilter::Game => 2,
            ReleaseTypeFilter::Crack => 3,
            ReleaseTypeFilter::Intro => 5,
            ReleaseTypeFilter::Graphics => 6,
            ReleaseTypeFilter::Diskmag => 10,
            ReleaseTypeFilter::Tool => 12,
            ReleaseTypeFilter::Custom(id) => *id,
        }
    }
    
    pub fn from_id(id: u32) -> Self {
        match id {
            1 => ReleaseTypeFilter::Demo,
            7 => ReleaseTypeFilter::Music,
            2 => ReleaseTypeFilter::Game,
            3 => ReleaseTypeFilter::Crack,
            5 => ReleaseTypeFilter::Intro,
            6 => ReleaseTypeFilter::Graphics,
            10 => ReleaseTypeFilter::Diskmag,
            12 => ReleaseTypeFilter::Tool,
            _ => ReleaseTypeFilter::Custom(id),
        }
    }
    
    pub fn name(&self) -> &str {
        match self {
            ReleaseTypeFilter::All => "All",
            ReleaseTypeFilter::Demo => "C64 Demo",
            ReleaseTypeFilter::Music => "C64 Music",
            ReleaseTypeFilter::Game => "C64 Game",
            ReleaseTypeFilter::Crack => "C64 Crack",
            ReleaseTypeFilter::Intro => "C64 Intro",
            ReleaseTypeFilter::Graphics => "C64 Graphics",
            ReleaseTypeFilter::Diskmag => "C64 Diskmag",
            ReleaseTypeFilter::Tool => "C64 Tool",
            ReleaseTypeFilter::Custom(_) => "Custom",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub query: String,
    pub r#type: Option<ReleaseTypeFilter>,
    pub groups: Vec<String>,
    pub sceners: Vec<String>,
    pub events: Vec<String>,
    pub years: Option<std::ops::Range<u32>>,
    pub min_rating: Option<f32>,
    pub max_rating: Option<f32>,
    pub countries: Vec<String>,
    pub credit_types: Vec<String>,
    pub has_download: Option<bool>,
    pub limit: usize,
    pub offset: usize,
    pub sort_by: SearchSort,
    pub sort_order: SortOrder,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            r#type: None,
            groups: Vec::new(),
            sceners: Vec::new(),
            events: Vec::new(),
            years: None,
            min_rating: None,
            max_rating: None,
            countries: Vec::new(),
            credit_types: Vec::new(),
            has_download: None,
            limit: 50,
            offset: 0,
            sort_by: SearchSort::Relevance,
            sort_order: SortOrder::Descending,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub total: u64,
    pub results: Vec<SearchResultItem>,
    pub facets: SearchFacets,
}

#[derive(Debug, Clone)]
pub struct SearchResultItem {
    pub release: Release,
    pub score: f32,
    pub highlights: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SearchFacets {
    pub groups: HashMap<String, u64>,
    pub sceners: HashMap<String, u64>,
    pub events: HashMap<String, u64>,
    pub years: HashMap<u32, u64>,
    pub countries: HashMap<String, u64>,
    pub types: HashMap<String, u64>,
    pub credit_types: HashMap<String, u64>,
}

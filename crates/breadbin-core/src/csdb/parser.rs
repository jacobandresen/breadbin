// parser - Parse CSDb webservice XML and HTML responses.

use crate::database::models::*;
use crate::core;

/// Extract website URL from XML, preferring itch.io URLs if multiple exist.
fn extract_website(xml: &str) -> String {
    // Find all Website tags
    let mut websites = Vec::new();
    let mut rest = xml;
    
    while let Some(start) = rest.find("<Website>") {
        let after_start = &rest[start + 9..]; // 9 = len("<Website>")
        if let Some(end) = after_start.find("</Website>") {
            let url = &rest[start + 9..start + 9 + end];
            let url = core::html_unescape(url.trim());
            websites.push(url.to_string());
            rest = &rest[start + 9 + end + 10..]; // 10 = len("</Website>")
        } else {
            break;
        }
    }
    
    // Prefer itch.io URLs
    for url in &websites {
        if url.contains("itch.io") {
            return url.clone();
        }
    }
    
    // Fall back to first website
    websites.first().cloned().unwrap_or_default()
}

/// Parse a release XML response from CSDb webservice.
pub fn parse_release(xml: &str, _id: u32) -> Result<Release, String> {
    // Extract basic release info
    let name = core::between(xml, "<Name>", "</Name>")
        .map(|s| core::html_unescape(s.trim()))
        .unwrap_or_default();
    
    let release_type = core::between(xml, "<Type>", "</Type>")
        .map(|s| core::html_unescape(s.trim()))
        .unwrap_or_default();
    
    let release_type_id = get_release_type_id(&release_type);
    
    let rating = core::between(xml, "<Rating>", "</Rating>")
        .and_then(|s| s.trim().parse::<f32>().ok())
        .unwrap_or(0.0);
    
    let screenshot_url = core::between(xml, "<ScreenShot>", "</ScreenShot>")
        .map(|s| core::html_unescape(s.trim()))
        .unwrap_or_default();
    
    let website = extract_website(xml);
    
    // Extract release date
    let release_day = core::between(xml, "<ReleaseDay>", "</ReleaseDay>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let release_month = core::between(xml, "<ReleaseMonth>", "</ReleaseMonth>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let release_year = core::between(xml, "<ReleaseYear>", "</ReleaseYear>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    
    // Extract achievement (compo and placing)
    let compo = core::between(xml, "<Compo>", "</Compo>")
        .map(|s| core::html_unescape(s.trim()));
    let place = core::between(xml, "<Place>", "</Place>")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    
    // Extract download count
    let download_count = core::between(xml, "<Downloads>", "</Downloads>")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    
    // Extract group info
    let group = parse_group_from_release(xml);
    let group_id = group.as_ref().map(|g| g.id);
    
    // Extract event info
    let event = parse_event_from_release(xml);
    let event_id = event.as_ref().map(|e| e.id);
    
    // Extract credits
    let credits = parse_credits(xml);
    
    // Extract download links
    let downloads = parse_download_links(xml);
    
    // Extract SID files
    let sids = parse_sids(xml);
    
    // Extract IA mappings (empty for now, would be populated separately)
    let ia_mappings = Vec::new();
    
    Ok(Release {
        id: _id,
        name,
        release_type_id,
        release_type,
        group_id,
        group: group.map(Box::new),
        rating,
        release_day,
        release_month,
        release_year,
        event_id,
        event: event.map(Box::new),
        compo,
        place,
        screenshot_url: Some(screenshot_url),
        screenshot_urls: Vec::new(),
        download_count,
        website: Some(website),
        credits,
        downloads,
        sids,
        ia_mappings,
        created_at: None,
        updated_at: None,
    })
}

/// Parse group information from release XML.
pub fn parse_group_from_release(xml: &str) -> Option<Group> {
    let group_xml = core::between(xml, "<ReleasedBy>", "</ReleasedBy>")?;
    let id = core::between(group_xml, "<ID>", "</ID>")
        .and_then(|s| s.trim().parse::<u32>().ok())?;
    let name = core::between(group_xml, "<Name>", "</Name>")
        .map(|s| core::html_unescape(s.trim()))?;
    let short = core::between(group_xml, "<Short>", "</Short>")
        .map(|s| core::html_unescape(s.trim()));
    let base_country = core::between(group_xml, "<BaseCountry>", "</BaseCountry>")
        .map(|s| core::html_unescape(s.trim()));
    let founded_month = core::between(group_xml, "<FoundMonth>", "</FoundMonth>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let founded_year = core::between(group_xml, "<FoundYear>", "</FoundYear>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let website = core::between(group_xml, "<Website>", "</Website>")
        .map(|s| core::html_unescape(s.trim()));
    let rating = core::between(group_xml, "<Rating>", "</Rating>")
        .and_then(|s| s.trim().parse::<f32>().ok())
        .unwrap_or(0.0);
    
    let grouptypes = core::between(group_xml, "<Grouptypes>", "</Grouptypes>")
        .map(|s| parse_grouptypes(s))
        .unwrap_or_default();
    
    let slogans = core::between(group_xml, "<Slogan>", "</Slogan>")
        .map(|s| vec![core::html_unescape(s.trim())])
        .or_else(|| {
            // Try to parse multiple slogans
            let mut slogans = Vec::new();
            let mut rest = group_xml;
            while let Some(slogan) = core::between(rest, "<Slogan>", "</Slogan>") {
                slogans.push(core::html_unescape(slogan.trim()));
                let pos = rest.find("</Slogan>").map(|p| p + 9).unwrap_or(rest.len());
                rest = &rest[pos..];
            }
            if slogans.is_empty() { None } else { Some(slogans) }
        })
        .unwrap_or_default();
    
    Some(Group {
        id,
        name,
        short,
        base_country,
        founded_month,
        founded_year,
        website,
        rating,
        group_types: grouptypes,
        slogans,
        updated_at: None,
    })
}

/// Parse grouptypes XML.
fn parse_grouptypes(xml: &str) -> Vec<String> {
    let mut types = Vec::new();
    let mut rest = xml;
    while let Some(ty) = core::between(rest, "<Grouptype>", "</Grouptype>") {
        types.push(core::html_unescape(ty.trim()));
        let pos = rest.find("</Grouptype>").map(|p| p + 13).unwrap_or(rest.len());
        rest = &rest[pos..];
    }
    types
}

/// Parse event information from release XML.
pub fn parse_event_from_release(xml: &str) -> Option<Event> {
    let event_xml = core::between(xml, "<ReleasedAt>", "</ReleasedAt>")?;
    let id = core::between(event_xml, "<ID>", "</ID>")
        .and_then(|s| s.trim().parse::<u32>().ok())?;
    let name = core::between(event_xml, "<Name>", "</Name>")
        .map(|s| core::html_unescape(s.trim()))?;
    let event_type = core::between(event_xml, "<EventType>", "</EventType>")
        .map(|s| core::html_unescape(s.trim()));
    
    let aka = core::between(event_xml, "<AKA>", "</AKA>")
        .map(|s| vec![core::html_unescape(s.trim())])
        .or_else(|| {
            let mut akas = Vec::new();
            let mut rest = event_xml;
            while let Some(aka_text) = core::between(rest, "<AKA>", "</AKA>") {
                akas.push(core::html_unescape(aka_text.trim()));
                let pos = rest.find("</AKA>").map(|p| p + 6).unwrap_or(rest.len());
                rest = &rest[pos..];
            }
            if akas.is_empty() { None } else { Some(akas) }
        })
        .unwrap_or_default();
    
    let start_day = core::between(event_xml, "<StartDay>", "</StartDay>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let start_month = core::between(event_xml, "<StartMonth>", "</StartMonth>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let start_year = core::between(event_xml, "<StartYear>", "</StartYear>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let end_day = core::between(event_xml, "<EndDay>", "</EndDay>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let end_month = core::between(event_xml, "<EndMonth>", "</EndMonth>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let end_year = core::between(event_xml, "<EndYear>", "</EndYear>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    
    let address = core::between(event_xml, "<Address>", "</Address>")
        .map(|s| core::html_unescape(s.trim()));
    let zip = core::between(event_xml, "<Zip>", "</Zip>")
        .map(|s| core::html_unescape(s.trim()));
    let city = core::between(event_xml, "<City>", "</City>")
        .map(|s| core::html_unescape(s.trim()));
    let country = core::between(event_xml, "<Country>", "</Country>")
        .map(|s| core::html_unescape(s.trim()));
    let website = core::between(event_xml, "<Website>", "</Website>")
        .map(|s| core::html_unescape(s.trim()));
    
    Some(Event {
        id,
        name,
        event_type,
        aka,
        start_day,
        start_month,
        start_year,
        end_day,
        end_month,
        end_year,
        address,
        zip,
        city,
        country,
        website,
        updated_at: None,
    })
}

/// Parse credits from release XML.
pub fn parse_credits(xml: &str) -> Vec<ReleaseCredit> {
    let mut credits = Vec::new();
    let credits_xml = core::between(xml, "<Credits>", "</Credits>")
        .unwrap_or_default();
    
    let mut rest = credits_xml;
    while let Some(credit_xml) = core::between(rest, "<Credit>", "</Credit>") {
        if let Some(credit) = parse_credit(credit_xml) {
            credits.push(credit);
        }
        let pos = rest.find("</Credit>").map(|p| p + 9).unwrap_or(rest.len());
        rest = &rest[pos..];
    }
    
    credits
}

/// Parse a single credit entry.
fn parse_credit(xml: &str) -> Option<ReleaseCredit> {
    let credit_type = core::between(xml, "<CreditType>", "</CreditType>")
        .map(|s| core::html_unescape(s.trim()))?;
    
    let handle_xml = core::between(xml, "<Handle>", "</Handle>")?;
    let handle = core::between(handle_xml, "<Handle>", "</Handle>")
        .map(|s| core::html_unescape(s.trim()))?;
    
    let handle_id: Option<u32> = core::between(handle_xml, "<ID>", "</ID>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    
    let currently_used = core::between(handle_xml, "<CurrentlyUsedHandle>", "</CurrentlyUsedHandle>")
        .map(|s| s.trim() == "true" || s.trim() == "1")
        .unwrap_or(true);
    
    // Parse scener info
    let scener_id: Option<u32> = core::between(handle_xml, "<Scener>", "</Scener>")
        .and_then(|scener_xml| {
            core::between(scener_xml, "<ID>", "</ID>")
                .and_then(|s| s.trim().parse::<u32>().ok())
        });
    
    // Parse functions
    let mut functions = Vec::new();
    let functions_xml = core::between(xml, "<FreelanceFunctions>", "</FreelanceFunctions>")
        .unwrap_or_default();
    let mut rest = functions_xml;
    while let Some(func) = core::between(rest, "<Function>", "</Function>") {
        functions.push(core::html_unescape(func.trim()));
        let pos = rest.find("</Function>").map(|p| p + 11).unwrap_or(rest.len());
        rest = &rest[pos..];
    }
    
    Some(ReleaseCredit {
        id: None,
        release_id: 0, // Will be set by caller
        scener_id,
        handle,
        credit_type,
        position: 0, // Will be set based on order
        functions,
    })
}

/// Parse download links from release XML.
pub fn parse_download_links(xml: &str) -> Vec<DownloadLink> {
    let mut downloads = Vec::new();
    let links_xml = core::between(xml, "<DownloadLinks>", "</DownloadLinks>")
        .unwrap_or_default();
    
    let mut rest = links_xml;
    while let Some(link_xml) = core::between(rest, "<DownloadLink>", "</DownloadLink>") {
        if let Some(download) = parse_download_link(link_xml) {
            downloads.push(download);
        }
        let pos = rest.find("</DownloadLink>").map(|p| p + 15).unwrap_or(rest.len());
        rest = &rest[pos..];
    }
    
    downloads
}

/// Parse a single download link.
fn parse_download_link(xml: &str) -> Option<DownloadLink> {
    let url = core::between(xml, "<Link>", "</Link>")
        .map(|s| core::html_unescape(s.trim()))?;
    let counter_url = core::between(xml, "<CounterLink>", "</CounterLink>")
        .map(|s| core::html_unescape(s.trim()));
    let downloads = core::between(xml, "<Downloads>", "</Downloads>")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let status = core::between(xml, "<Status>", "</Status>")
        .map(|s| core::html_unescape(s.trim()))
        .unwrap_or_else(|| "Ok".to_string());
    let is_internal = core::between(xml, "<Link>", "</Link>")
        .map(|s| s.contains("csdb.dk/getinternalfile.php"))
        .unwrap_or(false);
    
    // Infer file type from URL
    let file_type = if url.to_lowercase().ends_with(".zip") {
        Some("zip".to_string())
    } else if url.to_lowercase().ends_with(".d64") {
        Some("d64".to_string())
    } else if url.to_lowercase().ends_with(".d71") {
        Some("d71".to_string())
    } else if url.to_lowercase().ends_with(".d81") {
        Some("d81".to_string())
    } else if url.to_lowercase().ends_with(".t64") {
        Some("t64".to_string())
    } else if url.to_lowercase().ends_with(".tap") {
        Some("tap".to_string())
    } else if url.to_lowercase().ends_with(".prg") {
        Some("prg".to_string())
    } else if url.to_lowercase().ends_with(".crt") {
        Some("crt".to_string())
    } else if url.to_lowercase().ends_with(".g64") {
        Some("g64".to_string())
    } else if url.to_lowercase().ends_with(".nib") {
        Some("nib".to_string())
    } else if url.to_lowercase().ends_with(".p00") {
        Some("p00".to_string())
    } else if url.to_lowercase().ends_with(".x64") {
        Some("x64".to_string())
    } else if url.to_lowercase().ends_with(".sid") {
        Some("sid".to_string())
    } else {
        None
    };
    
    Some(DownloadLink {
        id: None,
        release_id: 0, // Will be set by caller
        url,
        counter_url,
        downloads,
        status,
        is_internal,
        file_type,
    })
}

/// Parse SID files from release XML.
pub fn parse_sids(xml: &str) -> Vec<SidFile> {
    let mut sids = Vec::new();
    let sids_xml = core::between(xml, "<UsedSIDs>", "</UsedSIDs>")
        .unwrap_or_default();
    
    let mut rest = sids_xml;
    while let Some(sid_xml) = core::between(rest, "<SID>", "</SID>") {
        if let Some(sid) = parse_sid(sid_xml) {
            sids.push(sid);
        }
        let pos = rest.find("</SID>").map(|p| p + 6).unwrap_or(rest.len());
        rest = &rest[pos..];
    }
    
    sids
}

/// Parse a single SID file entry.
fn parse_sid(xml: &str) -> Option<SidFile> {
    let sid_id = core::between(xml, "<ID>", "</ID>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let hvsc_path = core::between(xml, "<HVSCPath>", "</HVSCPath>")
        .map(|s| core::html_unescape(s.trim()));
    let name = core::between(xml, "<Name>", "</Name>")
        .map(|s| core::html_unescape(s.trim()))?;
    let author = core::between(xml, "<Author>", "</Author>")
        .map(|s| core::html_unescape(s.trim()));
    let released = core::between(xml, "<Released>", "</Released>")
        .map(|s| core::html_unescape(s.trim()));
    let load_addr = core::between(xml, "<LoadAddr>", "</LoadAddr>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let init_addr = core::between(xml, "<InitAddr>", "</InitAddr>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let play_addr = core::between(xml, "<PlayAddr>", "</PlayAddr>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let num_songs = core::between(xml, "<NOSongs>", "</NOSongs>")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(1);
    let default_song = core::between(xml, "<DefaultSong>", "</DefaultSong>")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(1);
    let sid_model = core::between(xml, "<SIDModel>", "</SIDModel>")
        .map(|s| core::html_unescape(s.trim()));
    let clock_speed = core::between(xml, "<ClockSpeed>", "</ClockSpeed>")
        .map(|s| core::html_unescape(s.trim()));
    let data_size = core::between(xml, "<DataSize>", "</DataSize>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    
    Some(SidFile {
        id: None,
        release_id: 0, // Will be set by caller
        sid_id,
        hvsc_path,
        name,
        author,
        released,
        load_addr,
        init_addr,
        play_addr,
        num_songs,
        default_song,
        sid_model,
        clock_speed,
        data_size,
    })
}

/// Get release type ID from type name.
pub fn get_release_type_id(type_name: &str) -> u32 {
    match type_name {
        "C64 Demo" => 1,
        "C64 Game" => 2,
        "C64 Crack" => 3,
        "C64 Trainer" => 4,
        "C64 Intro" => 5,
        "C64 Graphics" => 6,
        "C64 Music" => 7,
        "C64 Music Collection" => 8,
        "C64 Demo Collection" => 9,
        "C64 Diskmag" => 10,
        "C64 Papermag" => 11,
        "C64 Tool" => 12,
        "C64 Game Preview" => 13,
        "C64 Invitation" => 14,
        "C64 One-File Demo" => 15,
        "C64 4K Intro" => 16,
        "C64 8K Intro" => 17,
        "C64 16K Intro" => 18,
        "C64 32K Intro" => 19,
        "C64 64K Intro" => 20,
        _ => 0, // Unknown
    }
}

/// Parse a group XML response.
pub fn parse_group(xml: &str) -> Result<Group, String> {
    let id = core::between(xml, "<ID>", "</ID>")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .ok_or("Missing group ID")?;
    let name = core::between(xml, "<Name>", "</Name>")
        .map(|s| core::html_unescape(s.trim()))
        .ok_or("Missing group name")?;
    let short = core::between(xml, "<Short>", "</Short>")
        .map(|s| core::html_unescape(s.trim()));
    let base_country = core::between(xml, "<BaseCountry>", "</BaseCountry>")
        .map(|s| core::html_unescape(s.trim()));
    let founded_month = core::between(xml, "<FoundMonth>", "</FoundMonth>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let founded_year = core::between(xml, "<FoundYear>", "</FoundYear>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let website = core::between(xml, "<Website>", "</Website>")
        .map(|s| core::html_unescape(s.trim()));
    let rating = core::between(xml, "<Rating>", "</Rating>")
        .and_then(|s| s.trim().parse::<f32>().ok())
        .unwrap_or(0.0);
    
    let grouptypes = core::between(xml, "<Grouptypes>", "</Grouptypes>")
        .map(|s| parse_grouptypes(s))
        .unwrap_or_default();
    
    let mut slogans = Vec::new();
    let mut rest = xml;
    while let Some(slogan) = core::between(rest, "<Slogan>", "</Slogan>") {
        slogans.push(core::html_unescape(slogan.trim()));
        let pos = rest.find("</Slogan>").map(|p| p + 9).unwrap_or(rest.len());
        rest = &rest[pos..];
    }
    
    Ok(Group {
        id,
        name,
        short,
        base_country,
        founded_month,
        founded_year,
        website,
        rating,
        group_types: grouptypes,
        slogans,
        updated_at: None,
    })
}

/// Parse a scener XML response.
pub fn parse_scener(xml: &str) -> Result<Scener, String> {
    let id = core::between(xml, "<ID>", "</ID>")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .ok_or("Missing scener ID")?;
    
    let country = core::between(xml, "<Country>", "</Country>")
        .map(|s| core::html_unescape(s.trim()));
    let trivia = core::between(xml, "<Trivia>", "</Trivia>")
        .map(|s| core::html_unescape(s.trim()));
    let handle_story = core::between(xml, "<HandleStory>", "</HandleStory>")
        .map(|s| core::html_unescape(s.trim()));
    
    // Parse handles
    let mut handles = Vec::new();
    let handles_xml = core::between(xml, "<Handles>", "</Handles>")
        .unwrap_or_default();
    let mut rest = handles_xml;
    while let Some(handle_xml) = core::between(rest, "<Handle>", "</Handle>") {
        if let Some(handle) = parse_scener_handle(handle_xml, id) {
            handles.push(handle);
        }
        let pos = rest.find("</Handle>").map(|p| p + 9).unwrap_or(rest.len());
        rest = &rest[pos..];
    }
    
    // If no handles found, try to get the main handle
    if handles.is_empty() {
        if let Some(handle) = core::between(xml, "<Handle>", "</Handle>") {
            handles.push(ScenerHandle {
                id: None,
                scener_id: id,
                handle: core::html_unescape(handle.trim()),
                aka: None,
                currently_used: true,
                handle_story: None,
            });
        }
    }
    
    Ok(Scener {
        id,
        handles,
        country,
        trivia,
        handle_story,
        updated_at: None,
    })
}

/// Parse a scener handle XML.
fn parse_scener_handle(xml: &str, scener_id: u32) -> Option<ScenerHandle> {
    let handle_id = core::between(xml, "<ID>", "</ID>")
        .and_then(|s| s.trim().parse::<u64>().ok());
    let handle = core::between(xml, "<Handle>", "</Handle>")
        .map(|s| core::html_unescape(s.trim()))?;
    let aka = core::between(xml, "<AKA>", "</AKA>")
        .map(|s| core::html_unescape(s.trim()));
    let currently_used = core::between(xml, "<CurrentlyUsedHandle>", "</CurrentlyUsedHandle>")
        .map(|s| s.trim() == "true" || s.trim() == "1")
        .unwrap_or(true);
    let handle_story = core::between(xml, "<HandleStory>", "</HandleStory>")
        .map(|s| core::html_unescape(s.trim()));
    
    Some(ScenerHandle {
        id: handle_id,
        scener_id,
        handle,
        aka,
        currently_used,
        handle_story,
    })
}

/// Parse an event XML response.
pub fn parse_event(xml: &str) -> Result<Event, String> {
    let id = core::between(xml, "<ID>", "</ID>")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .ok_or("Missing event ID")?;
    let name = core::between(xml, "<Name>", "</Name>")
        .map(|s| core::html_unescape(s.trim()))
        .ok_or("Missing event name")?;
    let event_type = core::between(xml, "<EventType>", "</EventType>")
        .map(|s| core::html_unescape(s.trim()));
    
    let aka = core::between(xml, "<AKA>", "</AKA>")
        .map(|s| vec![core::html_unescape(s.trim())])
        .or_else(|| {
            let mut akas = Vec::new();
            let mut rest = xml;
            while let Some(aka_text) = core::between(rest, "<AKA>", "</AKA>") {
                akas.push(core::html_unescape(aka_text.trim()));
                let pos = rest.find("</AKA>").map(|p| p + 6).unwrap_or(rest.len());
                rest = &rest[pos..];
            }
            if akas.is_empty() { None } else { Some(akas) }
        })
        .unwrap_or_default();
    
    let start_day = core::between(xml, "<StartDay>", "</StartDay>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let start_month = core::between(xml, "<StartMonth>", "</StartMonth>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let start_year = core::between(xml, "<StartYear>", "</StartYear>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let end_day = core::between(xml, "<EndDay>", "</EndDay>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let end_month = core::between(xml, "<EndMonth>", "</EndMonth>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    let end_year = core::between(xml, "<EndYear>", "</EndYear>")
        .and_then(|s| s.trim().parse::<u32>().ok());
    
    let address = core::between(xml, "<Address>", "</Address>")
        .map(|s| core::html_unescape(s.trim()));
    let zip = core::between(xml, "<Zip>", "</Zip>")
        .map(|s| core::html_unescape(s.trim()));
    let city = core::between(xml, "<City>", "</City>")
        .map(|s| core::html_unescape(s.trim()));
    let country = core::between(xml, "<Country>", "</Country>")
        .map(|s| core::html_unescape(s.trim()));
    let website = core::between(xml, "<Website>", "</Website>")
        .map(|s| core::html_unescape(s.trim()));
    
    Ok(Event {
        id,
        name,
        event_type,
        aka,
        start_day,
        start_month,
        start_year,
        end_day,
        end_month,
        end_year,
        address,
        zip,
        city,
        country,
        website,
        updated_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_website_prefers_itchio() {
        let xml = r#"<Release>
            <Website>https://ausretrogamer.com/2022-reset64-4kb-craptastic-game-competition/</Website>
            <Website>https://jaammees.itch.io/circles</Website>
        </Release>"#;
        
        let result = extract_website(xml);
        assert!(result.contains("itch.io"), "Should prefer itch.io URL, got: {}", result);
    }

    #[test]
    fn test_extract_website_single() {
        let xml = r#"<Release>
            <Website>https://example.com</Website>
        </Release>"#;
        
        let result = extract_website(xml);
        assert_eq!(result, "https://example.com");
    }

    #[test]
    fn test_extract_website_no_itchio() {
        let xml = r#"<Release>
            <Website>https://ausretrogamer.com/</Website>
            <Website>https://example.com</Website>
        </Release>"#;
        
        let result = extract_website(xml);
        // Should return first website when no itch.io URL exists
        assert_eq!(result, "https://ausretrogamer.com/");
    }
}

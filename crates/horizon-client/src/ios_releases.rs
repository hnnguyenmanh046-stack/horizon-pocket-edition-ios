//! iOS: where Horizon PE's releases come out, and which of them is an update of this app: for
//! the update prompt (game/update_offer.rs).
//!
//! The app comes in three editions, released side by side in one repository: Horizon PE (tags
//! `v1.0.0`, `v1.0.1`, ...), Horizon PE: Shared Worlds, which adds shared worlds and voice
//! chat (tags `v1.0.0-shared-worlds`, ...), and Horizon PE: Shared Worlds + Mods, that one
//! built with fifteen native mods (tags `v1.0.0-shared-worlds-mods`, ...). An app is only ever
//! offered its own edition's releases: another's is another game to its player.
use crate::starling_releases::{Update, newer};

/// The repository the releases come out in, as GitHub's API names it.
pub const REPOSITORY: &str = "shopifygav222/horizon-pocket-edition-ios";

/// An edition of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition {
    /// Horizon PE.
    Standard,
    /// Horizon PE: Shared Worlds.
    SharedWorlds,
    /// Horizon PE: Shared Worlds + Mods.
    SharedWorldsMods,
}

/// The edition this source builds. The one line that differs between the branches `main`,
/// `shared-worlds` and `shared-worlds-mods` here: a merge from the branch below keeps this
/// branch's own.
pub const EDITION: Edition = Edition::SharedWorldsMods;

impl Edition {
    /// What follows the version in the edition's release tags.
    pub const fn tag_suffix(self) -> &'static str {
        match self {
            Self::Standard => "",
            Self::SharedWorlds => "-shared-worlds",
            Self::SharedWorldsMods => "-shared-worlds-mods",
        }
    }

    /// The edition's name, as its players read it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Standard => "Horizon PE",
            Self::SharedWorlds => "Horizon PE: Shared Worlds",
            Self::SharedWorldsMods => "Horizon PE: Shared Worlds + Mods",
        }
    }

    /// The version of a release tag of this edition (`v1.2.0-shared-worlds` is 1.2.0 of Shared
    /// Worlds): numbers and dots after a `v`, then the edition's suffix and nothing else.
    pub fn version_of(self, tag: &str) -> Option<&str> {
        let version = tag.strip_prefix('v')?.strip_suffix(self.tag_suffix())?;
        let plain = !version.is_empty()
            && version
                .split('.')
                .all(|n| !n.is_empty() && n.len() <= 9 && n.bytes().all(|b| b.is_ascii_digit()));
        plain.then_some(version)
    }
}

/// The repository's releases, newest first, as GitHub's API serves them.
pub fn releases_url() -> String {
    format!("https://api.github.com/repos/{REPOSITORY}/releases?per_page=50")
}

/// The newest release of `edition` in `releases` (the API's list) that comes after `current`
/// and has its .ipa. Drafts and pre-releases are never updates.
pub fn update_in(releases: &serde_json::Value, edition: Edition, current: &str) -> Option<Update> {
    let mut best: Option<Update> = None;
    for release in releases.as_array()? {
        if release["draft"].as_bool() != Some(false)
            || release["prerelease"].as_bool() != Some(false)
        {
            continue;
        }
        let Some(version) = release["tag_name"]
            .as_str()
            .and_then(|tag| edition.version_of(tag))
        else {
            continue;
        };
        if !newer(version, current) || best.as_ref().is_some_and(|b| !newer(version, &b.version)) {
            continue;
        }
        let Some(ipa) = release["assets"].as_array().and_then(|assets| {
            assets.iter().find(|a| {
                a["name"]
                    .as_str()
                    .is_some_and(|n| n.ends_with(".ipa") && n.len() > 4)
            })
        }) else {
            continue;
        };
        let (Some(name), Some(url)) = (
            ipa["name"].as_str(),
            ipa["browser_download_url"]
                .as_str()
                .filter(|u| u.starts_with("https://")),
        ) else {
            continue;
        };
        best = Some(Update {
            version: version.to_owned(),
            name: name.to_owned(),
            url: url.to_owned(),
            size: ipa["size"].as_u64().unwrap_or(0),
            sha256: ipa["digest"]
                .as_str()
                .and_then(|d| d.strip_prefix("sha256:"))
                .map(str::to_owned),
        });
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release(tag: &str, asset: &str) -> serde_json::Value {
        json!({
            "tag_name": tag,
            "draft": false,
            "prerelease": false,
            "assets": [
                {"name": "notes.txt", "browser_download_url": "https://example.test/notes.txt", "size": 9},
                {
                    "name": asset,
                    "browser_download_url": format!("https://example.test/{tag}/{asset}"),
                    "size": 123_456_789u64,
                    "digest": "sha256:abc123",
                },
            ],
        })
    }

    #[test]
    fn a_tag_belongs_to_one_edition_only() {
        let (standard, shared) = (Edition::Standard, Edition::SharedWorlds);
        assert_eq!(standard.version_of("v1.0.0"), Some("1.0.0"));
        assert_eq!(standard.version_of("v1.0.0-shared-worlds"), None);
        assert_eq!(shared.version_of("v1.0.0-shared-worlds"), Some("1.0.0"));
        assert_eq!(shared.version_of("v1.0.0"), None);
        for odd in [
            "1.0.0",
            "v",
            "v1..0",
            "v1.0.0-beta",
            "v1.0.0-shared-worlds-2",
            "native-builds",
            "v1.0.x",
            "",
        ] {
            assert_eq!(standard.version_of(odd), None, "{odd}");
            assert_eq!(shared.version_of(odd), None, "{odd}");
        }
    }

    #[test]
    fn each_edition_is_offered_its_own_newest_release() {
        let releases = json!([
            release("v1.2.0-shared-worlds", "HorizonPE-1.2.0-SharedWorlds.ipa"),
            release("v1.1.0", "HorizonPE-1.1.0.ipa"),
            release("v1.0.1", "HorizonPE-1.0.1.ipa"),
            release("v1.0.0-shared-worlds", "HorizonPE-1.0.0-SharedWorlds.ipa"),
            release("v1.0.0", "HorizonPE-1.0.0.ipa"),
        ]);
        let standard = update_in(&releases, Edition::Standard, "1.0.0").unwrap();
        assert_eq!(standard.version, "1.1.0");
        assert_eq!(standard.name, "HorizonPE-1.1.0.ipa");
        assert_eq!(
            standard.url,
            "https://example.test/v1.1.0/HorizonPE-1.1.0.ipa"
        );
        assert_eq!(standard.size, 123_456_789);
        assert_eq!(standard.sha256.as_deref(), Some("abc123"));
        let shared = update_in(&releases, Edition::SharedWorlds, "1.0.0").unwrap();
        assert_eq!(shared.version, "1.2.0");
        assert_eq!(shared.name, "HorizonPE-1.2.0-SharedWorlds.ipa");
        // Up to date: nothing, whatever the other edition has out.
        assert_eq!(update_in(&releases, Edition::Standard, "1.1.0"), None);
        assert_eq!(update_in(&releases, Edition::SharedWorlds, "1.2.0"), None);
        assert_eq!(update_in(&releases, Edition::Standard, "2.0.0"), None);
        // 1.10 comes after 1.9, number by number.
        let releases = json!([release("v1.9.0", "a.ipa"), release("v1.10.0", "b.ipa")]);
        assert_eq!(
            update_in(&releases, Edition::Standard, "1.0.0")
                .unwrap()
                .version,
            "1.10.0"
        );
    }

    #[test]
    fn drafts_prereleases_and_releases_without_an_ipa_are_not_updates() {
        let mut draft = release("v3.0.0", "HorizonPE-3.0.0.ipa");
        draft["draft"] = json!(true);
        let mut pre = release("v2.5.0", "HorizonPE-2.5.0.ipa");
        pre["prerelease"] = json!(true);
        let mut bare = release("v2.0.0", "HorizonPE-2.0.0.ipa");
        bare["assets"] =
            json!([{"name": "source.zip", "browser_download_url": "https://example.test/s.zip"}]);
        let mut plain_http = release("v1.5.0", "HorizonPE-1.5.0.ipa");
        plain_http["assets"][1]["browser_download_url"] = json!("http://example.test/x.ipa");
        let releases = json!([
            draft,
            pre,
            bare,
            plain_http,
            release("v1.1.0", "HorizonPE-1.1.0.ipa")
        ]);
        assert_eq!(
            update_in(&releases, Edition::Standard, "1.0.0")
                .unwrap()
                .version,
            "1.1.0"
        );
        // Not a list (an error's body), or an empty one: nothing.
        assert_eq!(
            update_in(&json!({"message": "rate limited"}), EDITION, "1.0.0"),
            None
        );
        assert_eq!(update_in(&json!([]), EDITION, "1.0.0"), None);
    }

    /// This branch is Horizon PE: Shared Worlds + Mods; `main` has this test say Horizon PE,
    /// `shared-worlds` Shared Worlds.
    #[test]
    fn this_source_is_the_shared_worlds_and_mods_edition() {
        assert_eq!(EDITION, Edition::SharedWorldsMods);
        assert_eq!(EDITION.tag_suffix(), "-shared-worlds-mods");
    }

    /// The three editions' tags never read as one another's.
    #[test]
    fn the_mods_editions_tags_are_its_own() {
        let (standard, shared, mods) = (
            Edition::Standard,
            Edition::SharedWorlds,
            Edition::SharedWorldsMods,
        );
        assert_eq!(mods.version_of("v1.2.0-shared-worlds-mods"), Some("1.2.0"));
        assert_eq!(mods.version_of("v1.2.0-shared-worlds"), None);
        assert_eq!(mods.version_of("v1.2.0"), None);
        assert_eq!(shared.version_of("v1.2.0-shared-worlds-mods"), None);
        assert_eq!(standard.version_of("v1.2.0-shared-worlds-mods"), None);
        let releases = json!([
            release("v1.3.0-shared-worlds", "a.ipa"),
            release("v1.2.0-shared-worlds-mods", "HorizonPE-1.2.0-SharedWorlds-Mods.ipa"),
            release("v1.1.0", "b.ipa"),
        ]);
        let update = update_in(&releases, mods, "1.0.0").unwrap();
        assert_eq!(update.version, "1.2.0");
        assert_eq!(update.name, "HorizonPE-1.2.0-SharedWorlds-Mods.ipa");
        assert_eq!(update_in(&releases, shared, "1.0.0").unwrap().version, "1.3.0");
        assert_eq!(update_in(&releases, standard, "1.0.0").unwrap().version, "1.1.0");
    }
}

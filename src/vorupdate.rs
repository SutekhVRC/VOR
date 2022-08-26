use reqwest::{self, StatusCode};
use serde_json::Value;
use std::os::windows::process::CommandExt;
use std::process::Command;

const DETACHED_PROCESS: u32 = 0x00000008;
pub const VERSION: &str = "0.2.0-beta";

pub struct VORVersion {
    pub version_str: String,
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

pub struct VORUpdater {
    pub current_version: Option<VORVersion>,
    pub newest_version: Option<VORVersion>,
    pub up_to_date: bool,
    pub release_blob: Option<Value>,
}

impl VORUpdater {
    pub fn new() -> Self {
        let mut vu = Self {
            current_version: None,
            newest_version: None,
            up_to_date: false,
            release_blob: None,
        };

        vu.current_version = Self::parse_vor_version_from_str(VERSION);

        let unr = vu.update_newest_release();
        if unr < 0 {
            // Failed to update newest release
            //println!("Failed to update newest: {}", unr);
        }

        vu.up_to_date();
        vu
    }

    pub fn update_vor(blob: Value) {
        //println!("BLOB: {:?}", blob);

        let msi_download_url = blob.as_array().unwrap()[0]
            .get("assets")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .get("browser_download_url")
            .unwrap()
            .as_str()
            .unwrap();

        let _ = Command::new("msiexec")
            .args(["/i", msi_download_url, "/n", "/passive"])
            .creation_flags(DETACHED_PROCESS)
            .spawn();
    }

    fn up_to_date(&mut self) {
        if let None = self.newest_version {
            self.up_to_date = true;
            println!("Up to date bc failed to get newest!");
        } else {
            if self.current_version.as_ref().unwrap().version_str
                == self.newest_version.as_ref().unwrap().version_str
            {
                self.up_to_date = true;
            } else {
                self.up_to_date = false;
            }
        }
    }

    fn update_newest_release(&mut self) -> i8 {
        let http_cli = reqwest::blocking::Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:104.0) Gecko/20100101 Firefox/104.0",
            )
            .build()
            .unwrap();
        match http_cli
            .get("https://api.github.com/repos/SutekhVRC/VOR/releases?per_page=1")
            .send()
        {
            Ok(res) => {
                if res.status() == StatusCode::OK {
                    let res_str = res.text().unwrap();
                    let release_response: Value = serde_json::from_str(res_str.as_str()).unwrap();
                    let json_val_str = release_response.as_array().unwrap()[0]
                        .get("tag_name")
                        .unwrap()
                        .as_str()
                        .unwrap();
                    match Self::parse_vor_version_from_str(json_val_str) {
                        Some(new) => {
                            self.newest_version = Some(new);
                            self.release_blob = Some(release_response);
                            return 0;
                        }
                        None => return -3,
                    }
                } else {
                    println!("{:?}", res.text());
                    return -2;
                }
            }
            Err(_err) => return -1,
        }
    }

    fn parse_vor_version_from_str(version: &str) -> Option<VORVersion> {
        let release_split: Vec<&str> = version.split("-").into_iter().collect();
        //println!("REL SPLIT: {:?}", release_split);
        let version_split: Vec<&str> = release_split[0].split(".").into_iter().collect();
        //println!("VER SPLIT: {:?}", version_split);

        let mut new_version = VORVersion {
            version_str: version.to_string(),
            major: 0,
            minor: 0,
            patch: 0,
        };

        if version_split.len() == 3 {
            new_version.major = version_split[0].parse().unwrap_or(0);
            new_version.minor = version_split[1].parse().unwrap_or(0);
            new_version.patch = version_split[2].parse().unwrap_or(0);
            if (new_version.major + new_version.minor + new_version.patch) == 0 {
                return None;
            }
            new_version.version_str = version.to_string();
            return Some(new_version);
        } else {
            return None;
        }
    }
}

#[cfg(target_os = "linux")]
use open;
use reqwest::{self, StatusCode};
use serde_json::Value;
#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
pub const DETACHED_PROCESS: u32 = 0x00000008;

#[cfg(target_os = "windows")]
pub const VERSION: &str = "0.3.3-beta-windows";

#[cfg(target_os = "linux")]
pub const VERSION: &str = "0.3.3-beta-linux";

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

    #[cfg(target_os = "linux")]
    pub fn update_vor(blob: Value) {
        let assets: &Vec<Value> = blob.as_array().unwrap()[0]
            .get("assets")
            .unwrap()
            .as_array()
            .unwrap();
        for asset in assets {
            let dl_url = asset.get("browser_download_url").unwrap().as_str().unwrap();
            if dl_url.ends_with(".elf") {
                open::that(dl_url).unwrap();
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub fn update_vor(blob: Value) {
        let assets: &Vec<Value> = blob.as_array().unwrap()[0]
            .get("assets")
            .unwrap()
            .as_array()
            .unwrap();
        for asset in assets {
            let dl_url = asset.get("browser_download_url").unwrap().as_str().unwrap();
            if dl_url.ends_with(".msi") {
                Command::new("msiexec")
                    .args(["/i", dl_url, "/n", "/passive"])
                    .creation_flags(DETACHED_PROCESS)
                    .spawn()
                    .unwrap();
            }
        }
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
        let version_split: Vec<&str> = release_split[0].split(".").into_iter().collect();

        let mut version_parse = VORVersion {
            version_str: String::new(),
            major: 0,
            minor: 0,
            patch: 0,
        };

        if version_split.len() == 3 {
            version_parse.major = version_split[0].parse().unwrap_or(0);
            version_parse.minor = version_split[1].parse().unwrap_or(0);
            version_parse.patch = version_split[2].parse().unwrap_or(0);

            if (version_parse.major + version_parse.minor + version_parse.patch) == 0 {
                return None;
            }

            version_parse.version_str = release_split[0].to_string();

            return Some(version_parse);
        } else {
            return None;
        }
    }
}

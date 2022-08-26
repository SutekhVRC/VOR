#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::{egui::Vec2, run_native, NativeOptions};

use clap::Parser;

mod config;
mod routing;
mod ui;
mod vorerr;
mod vorupdate;
mod vorutils;

use config::config_construct;
use ui::VORGUI;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct VCArgs {
    #[clap(short, long)]
    pub enable_on_start: bool,
}

fn parse_args() -> VCArgs {
    VCArgs::parse()
}

fn main() {
    let args = parse_args();
    //println!("Enable On Start: {}", args.enable_on_start);
    let (vor_router_config, configs, pf) = config_construct();

    let mut native_opts = NativeOptions::default();
    native_opts.initial_window_size = Some(Vec2::new(330., 450.));

    run_native(
        "VRChat OSC Router",
        native_opts,
        Box::new(|cc| Box::new(VORGUI::new(cc, args, configs, vor_router_config, pf))),
    );
}

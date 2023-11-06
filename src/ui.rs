use crate::config::vor_root;
use crate::pf::PacketFilter;
use crate::routedbg::DebugPacket;
use crate::vorutils::get_user_home_dir;
use crate::VCArgs;
use crate::{
    config::{
        AppConfigCheck, AppConfigState, AppConflicts, InputValidation, RouterConfig,
        VORAppIdentifier, VORAppStatus, VORConfig, VORConfigWrapper,
    },
    routedbg,
    routing::{route_main, RouterMsg},
    vorupdate::{VORUpdater, VERSION},
    vorutils::{check_valid_ipv4, check_valid_port, file_exists},
};

use eframe::egui::{
    Context, Hyperlink, Label, Layout, RichText, ScrollArea, Style, TextStyle, TopBottomPanel,
    Visuals,
};
use eframe::epaint::Color32;
use eframe::{
    egui::{self, CentralPanel},
    App,
};
use rosc::OscPacket;
use std::sync::mpsc::{self, Receiver, Sender};
use std::{fs, thread};

pub struct VORGUI {
    configs: Vec<(VORConfigWrapper, VORAppStatus, AppConfigState)>,
    vc_args: VCArgs,
    running: VORExecutionState,
    tab: VORGUITab,
    router_channel: Option<Sender<RouterMsg>>,
    vor_router_config: RouterConfig,
    adding_new_app: bool,
    new_app: Option<VORConfigWrapper>,
    new_app_cf_exists_err: AppConfigCheck,
    router_msg_recvr: Option<Receiver<VORAppIdentifier>>,
    pf: PacketFilter,
    pf_wl_new: (String, bool),
    pf_bl_new: (String, bool),
    update_engine: VORUpdater,
    route_debug: Option<routedbg::VORDebug>,
}

enum VORExecutionState {
    Running,
    Stopped,
    Error(String),
}

pub enum VORGUITab {
    Main,
    Apps,
    Config,
    Firewall,
}

impl VORGUI {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        vc_args: VCArgs,
        configs: Vec<(VORConfigWrapper, VORAppStatus, AppConfigState)>,
        vor_router_config: RouterConfig,
        pf: PacketFilter,
    ) -> Self {
        let mut app_obj = VORGUI {
            configs,
            vc_args,
            running: VORExecutionState::Stopped,
            tab: VORGUITab::Main,
            router_channel: None,
            vor_router_config,
            adding_new_app: false,
            new_app: None,
            new_app_cf_exists_err: AppConfigCheck::SUCCESS,
            router_msg_recvr: None,
            pf,
            pf_bl_new: (String::new(), false),
            pf_wl_new: (String::new(), false),
            update_engine: VORUpdater::new(),
            route_debug: None,
        };

        // Read config values
        // Set fonts etc.
        let mut style: Style = (*cc.egui_ctx.style()).clone();
        style.override_text_style = Some(TextStyle::Monospace);
        cc.egui_ctx.set_style(style);
        let visuals = Visuals::dark();
        cc.egui_ctx.set_visuals(visuals);

        // Enable on start flag
        if app_obj.vc_args.enable_on_start {
            app_obj.start_router();
        }

        return app_obj;
    }

    fn update_vor(&mut self) {
        if let VORExecutionState::Running = self.running {
            self.stop_router();
        }
        let blob = self.update_engine.release_blob.take().unwrap();

        thread::spawn(move || {
            VORUpdater::update_vor(blob);
        });
        thread::sleep(std::time::Duration::from_secs(1));
        std::process::exit(0);
    }

    fn set_tab(&mut self, ctx: &Context) {
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.with_layout(Layout::left_to_right(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(RichText::new("Main").monospace()).clicked() {
                            self.tab = VORGUITab::Main;
                        }
                        ui.separator();

                        if ui.button(RichText::new("Apps").monospace()).clicked() {
                            self.tab = VORGUITab::Apps;
                        }
                        ui.separator();

                        if ui.button(RichText::new("PF").monospace()).clicked() {
                            self.tab = VORGUITab::Firewall;
                        }
                        ui.separator();

                        if ui.button(RichText::new("Config").monospace()).clicked() {
                            self.tab = VORGUITab::Config;
                        }

                        //ui.separator();
                        ui.with_layout(Layout::right_to_left(), |ui| {
                            match self.running {
                                VORExecutionState::Running => {ui.label(RichText::new("Routing").color(Color32::GREEN));},
                                VORExecutionState::Stopped => {ui.label(RichText::new("Stopped").color(Color32::RED));},
                                VORExecutionState::Error(ref e) => {
                                    ui.label(RichText::new(format!("Error: {}", e)).color(Color32::RED));
                                }
                            }

                            if !self.update_engine.up_to_date {
                                if ui
                                    .button(
                                        RichText::new("Update").color(Color32::GREEN).monospace(),
                                    )
                                    .clicked()
                                {
                                    self.update_vor();
                                    std::thread::sleep(std::time::Duration::from_secs(5));
                                }
                            }
                        });
                        //ui.separator();
                    });
                    //ui.separator();
                });
            });
        });
    }

    fn debug_window(&mut self, ui: &egui::Ui, ctx: &egui::Context) {
        ctx.request_repaint();
        egui::Window::new("VOR Debug")
            .resizable(true)
            .drag_bounds(ui.max_rect())
            .min_height(450.)
            .min_width(425.)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(
                        &mut self.route_debug.as_mut().unwrap().ui_opts.show_incoming,
                        "INCOMING",
                    );
                    ui.checkbox(
                        &mut self.route_debug.as_mut().unwrap().ui_opts.show_outgoing,
                        "OUTGOING",
                    );
                    ui.checkbox(
                        &mut self.route_debug.as_mut().unwrap().ui_opts.show_allowed,
                        "ALLOWED",
                    );
                    ui.checkbox(
                        &mut self.route_debug.as_mut().unwrap().ui_opts.show_dropped,
                        "DROPPED",
                    );
                    ui.with_layout(Layout::right_to_left(), |ui| {
                        if ui.button("Clear Packets").clicked() {
                            self.route_debug.as_mut().unwrap().vor_dbg_packets.clear();
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Search/Filter: ");
                    ui.text_edit_singleline(
                        &mut self.route_debug.as_mut().unwrap().ui_opts.search_query,
                    );
                });

                ui.separator();
                ScrollArea::new([true, true])
                    .id_source("top_debug_section")
                    .max_height(ui.available_height())
                    .auto_shrink([false, false])
                    .stick_to_right()
                    .show(ui, |ui| {
                        let mut id_increment = 0;
                        for packet in &self.route_debug.as_ref().unwrap().vor_dbg_packets {
                            match packet {
                                DebugPacket::INCOMING(pkt) => {
                                    if self.route_debug.as_ref().unwrap().ui_opts.show_incoming {
                                        if self.route_debug.as_ref().unwrap().ui_opts.show_allowed
                                            && pkt.mode.is_allowed()
                                        {
                                            //println!("PRINTING ALLOWED");

                                            if packet.search(
                                                self.route_debug
                                                    .as_ref()
                                                    .unwrap()
                                                    .ui_opts
                                                    .search_query
                                                    .clone(),
                                            ) || self
                                                .route_debug
                                                .as_ref()
                                                .unwrap()
                                                .ui_opts
                                                .search_query
                                                .is_empty()
                                            {
                                                egui::CollapsingHeader::new(
                                                    RichText::new(format!(
                                                        "Incoming({:?}): {}",
                                                        pkt.mode, pkt.from_address
                                                    ))
                                                    .color(Color32::GREEN),
                                                )
                                                .id_source(id_increment)
                                                .show(ui, |ui| {
                                                    //ui.label(format!("Mode {:?} - Buffer length: {} - OscPacket: {:?}", pkt.mode, pkt.packet_buffer.len(), pkt.osc_packet));

                                                    ui.label(format!(
                                                        "L3 Src Address: {}",
                                                        pkt.from_address
                                                    ));
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.label(RichText::new("PF Decision:"));
                                                        ui.colored_label(
                                                            Color32::GREEN,
                                                            RichText::new(format!(
                                                                "{:?}",
                                                                pkt.mode
                                                            )),
                                                        );
                                                    });

                                                    if ui.button("Copy OSC Address").clicked() {
                                                        ui.output().copied_text =
                                                            match pkt.osc_packet.as_ref().unwrap() {
                                                                OscPacket::Message(msg) => {
                                                                    msg.addr.clone()
                                                                }
                                                                OscPacket::Bundle(_) => {
                                                                    "".to_string()
                                                                }
                                                            }
                                                    }
                                                    egui::CollapsingHeader::new(RichText::new(
                                                        "OSC Packet",
                                                    ))
                                                    .show(ui, |ui| {
                                                        ui.label(RichText::new(format!(
                                                            "{:#?}",
                                                            pkt.osc_packet.as_ref().unwrap()
                                                        )));
                                                    });
                                                });
                                                id_increment += 1;
                                            }
                                        }

                                        if self.route_debug.as_ref().unwrap().ui_opts.show_dropped
                                            && pkt.mode.is_dropped()
                                        {
                                            //println!("PRINTING DROPPED");

                                            if packet.search(
                                                self.route_debug
                                                    .as_ref()
                                                    .unwrap()
                                                    .ui_opts
                                                    .search_query
                                                    .clone(),
                                            ) || self
                                                .route_debug
                                                .as_ref()
                                                .unwrap()
                                                .ui_opts
                                                .search_query
                                                .is_empty()
                                            {
                                                egui::CollapsingHeader::new(
                                                    RichText::new(format!(
                                                        "Incoming({:?}): {}",
                                                        pkt.mode, pkt.from_address
                                                    ))
                                                    .color(Color32::RED),
                                                )
                                                .id_source(id_increment)
                                                .show(ui, |ui| {
                                                    //ui.label(format!("Mode {:?} - Buffer length: {} - OscPacket: {:?}", pkt.mode, pkt.packet_buffer.len(), pkt.osc_packet));

                                                    ui.label(format!(
                                                        "L3 Src Address: {}",
                                                        pkt.from_address
                                                    ));
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.label(RichText::new("PF Decision:"));
                                                        ui.colored_label(
                                                            Color32::RED,
                                                            RichText::new(format!(
                                                                "{:?}",
                                                                pkt.mode
                                                            )),
                                                        );
                                                    });
                                                    if ui.button("Copy OSC Address").clicked() {
                                                        ui.output().copied_text =
                                                            match pkt.osc_packet.as_ref().unwrap() {
                                                                OscPacket::Message(msg) => {
                                                                    msg.addr.clone()
                                                                }
                                                                OscPacket::Bundle(_) => {
                                                                    "".to_string()
                                                                }
                                                            }
                                                    }
                                                    egui::CollapsingHeader::new(RichText::new(
                                                        "OSC Packet",
                                                    ))
                                                    .show(ui, |ui| {
                                                        ui.label(RichText::new(format!(
                                                            "{:#?}",
                                                            pkt.osc_packet.as_ref().unwrap()
                                                        )));
                                                    });
                                                });
                                                id_increment += 1;
                                            }
                                        }
                                    }
                                }
                                DebugPacket::OUTGOING(pkt) => {
                                    if self.route_debug.as_ref().unwrap().ui_opts.show_outgoing {
                                        if packet.search(
                                            self.route_debug
                                                .as_ref()
                                                .unwrap()
                                                .ui_opts
                                                .search_query
                                                .clone(),
                                        ) || self
                                            .route_debug
                                            .as_ref()
                                            .unwrap()
                                            .ui_opts
                                            .search_query
                                            .is_empty()
                                        {
                                            egui::CollapsingHeader::new(
                                                RichText::new(format!(
                                                    "Outgoing: {} ({})",
                                                    pkt.route, pkt.to_address
                                                ))
                                                .color(Color32::from_rgb(0xef, 0x98, 0xff)),
                                            )
                                            .id_source(id_increment)
                                            .show(
                                                ui,
                                                |ui| {
                                                    //ui.label(format!("App Route: {} - Buffer length: {} - OscPacket: {:?}", pkt.route, pkt.packet_buffer.len(), pkt.osc_packet));
                                                    ui.label(format!(
                                                        "L3 Dest Address: {}",
                                                        pkt.to_address
                                                    ));
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.label(RichText::new("Route:"));
                                                        ui.colored_label(
                                                            Color32::GREEN,
                                                            RichText::new(format!("{}", pkt.route)),
                                                        );
                                                    });

                                                    if ui.button("Copy OSC Address").clicked() {
                                                        ui.output().copied_text =
                                                            match pkt.osc_packet.as_ref().unwrap() {
                                                                OscPacket::Message(msg) => {
                                                                    msg.addr.clone()
                                                                }
                                                                OscPacket::Bundle(_) => {
                                                                    "".to_string()
                                                                }
                                                            }
                                                    }
                                                    egui::CollapsingHeader::new(RichText::new(
                                                        "OSC Packet",
                                                    ))
                                                    .show(ui, |ui| {
                                                        ui.label(RichText::new(format!(
                                                            "{:#?}",
                                                            pkt.osc_packet.as_ref().unwrap()
                                                        )));
                                                    });
                                                },
                                            );
                                            id_increment += 1;
                                        }
                                    }
                                }
                            }
                        }
                    });
            });
    }

    fn debug_status_refresh(&mut self) {
        // Update VOR Debug structure
        if self.route_debug.is_some() {
            for _ in 0..256 {
                match self
                    .route_debug
                    .as_ref()
                    .unwrap()
                    .sig_channel_handler
                    .debug_in
                    .rx
                    .try_recv()
                {
                    Ok(dbg_pkt) => {
                        self.route_debug
                            .as_mut()
                            .unwrap()
                            .vor_dbg_packets
                            .insert(0, dbg_pkt);
                        self.route_debug.as_mut().unwrap().vor_dbg_packets.truncate(8192);
                    }
                    Err(_err) => break, /* Failed to read from debug channel */
                }
            }
        }
    }

    fn status_refresh(&mut self) {
        let status = match self.router_msg_recvr.as_ref() {
            Some(recvr) => match recvr.try_recv() {
                Ok(status) => status,
                Err(_e) => {
                    return;
                }
            },
            None => return,
        };
        if status.index == -1 {
            println!("[!] VOR failed to bind listener socket.. Not started!");
            self.running = VORExecutionState::Error("VOR Bind Error".to_string());
        } else {
            self.configs[status.index as usize].1 = status.status;
        }
    }

    fn status(&mut self, ui: &mut egui::Ui) {
        //update vor status
        self.status_refresh();

        ScrollArea::new([false, true]).show(ui, |ui| {
            // App Statuses
            if self.configs.len() > 0 {
                for i in 0..self.configs.len() {
                    let mut status_color = Color32::GREEN;
                    match self.configs[i].1 {
                        VORAppStatus::Running => {}
                        VORAppStatus::Stopped => status_color = Color32::RED,
                        VORAppStatus::AppError(_) => status_color = Color32::GOLD,
                        VORAppStatus::Disabled => status_color = Color32::RED,
                    }
                    ui.horizontal(|ui| {
                        ui.group(|ui| {
                            ui.label(format!("{}", self.configs[i].0.config_data.app_name));
                            ui.with_layout(Layout::right_to_left(), |ui| {
                                ui.separator();
                                ui.add(
                                    Label::new(
                                        RichText::new(format!("{}", self.configs[i].1))
                                            .color(status_color),
                                    )
                                    .wrap(true),
                                );
                            });
                        });
                    });
                }
            }
        });
    }

    fn list_vor_config(&mut self, ui: &mut egui::Ui) {
        // UI for VOR config
        ui.add_space(1.0);
        ui.label("Networking");
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label("Bind Host: ");
            ui.add(egui::TextEdit::singleline(
                &mut self.vor_router_config.bind_host,
            ));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Bind Port: ");
            ui.add(egui::TextEdit::singleline(
                &mut self.vor_router_config.bind_port,
            ));
        });
        /* For feature never ended up adding
        ui.horizontal_wrapped(|ui| {
            ui.label("VRChat Host: ");
            ui.add(egui::TextEdit::singleline(
                &mut self.vor_router_config.vrc_host,
            ));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("VRChat Port: ");
            ui.add(egui::TextEdit::singleline(
                &mut self.vor_router_config.vrc_port,
            ));
        });
        */
        ui.horizontal_wrapped(|ui| {
            ui.label("VOR Buffer Queue Size: ");
            ui.add(egui::TextEdit::singleline(
                &mut self.vor_router_config.vor_buffer_size,
            ));
        });

        ui.separator();
        ui.add_space(1.0);
        ui.label("Routing mode");
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut self.vor_router_config.async_mode,
                "Asynchronous routing",
            )
        });
    }

    fn router_exec_button(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(), |ui| {
                /* meh
                if ui.button("Reload").clicked() {
                    // Reload app configs and VOR config and restart all threads
                    // Send ShutdownAll
                    // Reload configs
                    // Start router thread
                    if self.running {
                        self.stop_router();
                        let (router_config, app_configs) = config_construct();
                        self.vor_router_config = router_config;
                        self.configs = app_configs;
                        self.start_router();
                    }
                }*/
                match self.running {
                    //ui.group(|ui| {
                    VORExecutionState::Running => {
                        if ui.button("Stop").clicked() {
                            if let VORExecutionState::Running = self.running {
                                self.stop_router();
                                ctx.request_repaint();
                            }
                        }

                        if self.route_debug.is_some() {
                            ui.label(RichText::new("Debug Mode Active").color(Color32::GREEN));
                        }
                    
                    },
                    VORExecutionState::Stopped => {
                    //ui.group(|ui| {
                        if ui
                            .button(RichText::new("Start").color(Color32::GREEN))
                            .clicked()
                        {
                            if let VORExecutionState::Stopped = self.running {
                                self.start_router();
                                ctx.request_repaint();
                            }
                        }
                        if self.route_debug.is_some() {
                            if ui
                                .button(RichText::new("Debug").color(Color32::GREEN))
                                .clicked()
                            {
                                self.route_debug = None;
                            }
                        } else {
                            if ui
                                .button(RichText::new("Debug").color(Color32::RED))
                                .clicked()
                            {
                                self.route_debug = Some(routedbg::VORDebug::new());
                            }
                        }
                    //});
                    },
                    VORExecutionState::Error(_) => {
                        if ui.button("Stop").clicked() {
                            //if let VORExecutionState::Running = self.running {
                                self.stop_router();
                                ctx.request_repaint();
                            //}
                        }

                        if self.route_debug.is_some() {
                            ui.label(RichText::new("Debug Mode Active").color(Color32::GREEN));
                        }
                    }
                }
            });
            ui.separator();
        });
    }

    fn start_router(&mut self) {
        // Create main router thread - 1 channel store TX in GUI object
        // router thread recv msgs from GUI thread and controls child threads each with their own channel to comm with router thread
        // Generate / Start OSC threads here
        let mut ids = -1;
        let confs: Vec<(VORConfig, i64)> = self
            .configs
            .iter()
            .filter_map(|c| {
                if let VORAppStatus::Disabled = c.1 {
                    ids += 1;
                    None
                } else {
                    ids += 1;
                    Some((c.0.config_data.clone(), ids))
                }
            })
            .collect();

        let (router_tx, router_rx): (Sender<RouterMsg>, Receiver<RouterMsg>) = mpsc::channel();
        let (app_stat_tx, app_stat_rx): (Sender<VORAppIdentifier>, Receiver<VORAppIdentifier>) =
            mpsc::channel();
        self.router_channel = Some(router_tx);
        self.router_msg_recvr = Some(app_stat_rx);

        let bind_target = format!(
            "{}:{}",
            self.vor_router_config.bind_host, self.vor_router_config.bind_port
        );
        let vor_buf_size = match self.vor_router_config.vor_buffer_size.parse::<usize>() {
            Ok(s) => s,
            Err(_) => {
                self.router_channel = None;
                //self.router_msg_recvr = None;
                return;
            }
        };

        let pf = self.pf.clone();
        let async_mode = self.vor_router_config.async_mode;

        let debug_sender = match &self.route_debug {
            Some(rd) => Some(rd.sig_channel_handler.debug_in.tx.clone()),
            None => None,
        };
        let debug_config = match &self.route_debug {
            Some(rd) => Some(rd.options.clone()),
            None => None,
        };

        thread::spawn(move || {
            route_main(
                bind_target,
                router_rx,
                app_stat_tx,
                confs,
                pf,
                vor_buf_size,
                async_mode,
                debug_sender,
                debug_config,
            );
        });

        self.running = VORExecutionState::Running;
    }

    fn stop_router(&mut self) {
        // Send shutdown signal to OSC threads here
        match self.router_channel
            .take()
            .unwrap()
            .send(RouterMsg::ShutdownAll)
            {
                Ok(_) => {},
                Err(_e) => {/* If this happens its likely VOR failed to bind or initialize route routines */}
            }

        self.running = VORExecutionState::Stopped;
        thread::sleep(std::time::Duration::from_secs(1));

        if self.vor_router_config.async_mode {
            for app_conf in &mut self.configs {
                if let VORAppStatus::Running = app_conf.1 {
                    app_conf.1 = VORAppStatus::Stopped;
                }
            }
        }
    }

    fn save_vor_config(&mut self) {
        #[cfg(target_os = "windows")]
        {
            fs::write(
                format!(
                    "{}\\VORConfig.json",
                    vor_root().expect("[-] Roaming directory can't be found!")
                ),
                serde_json::to_string(&self.vor_router_config).unwrap(),
            )
            .unwrap();
        }

        #[cfg(target_os = "linux")]
        {
            fs::write(
                format!("{}/.vor/VORConfig.json", get_user_home_dir()),
                serde_json::to_string(&self.vor_router_config).unwrap(),
            )
            .unwrap();
        }
    }

    fn save_app_config(&mut self, app_index: usize, add_new: bool) -> AppConfigCheck {
        match self.check_app_inputs(app_index) {
            InputValidation::CLEAN => {}
            InputValidation::AH(s) => {
                if add_new {
                    self.configs.pop();
                }
                return AppConfigCheck::IV(InputValidation::AH(s));
            }
            InputValidation::AP(s) => {
                if add_new {
                    self.configs.pop();
                }
                return AppConfigCheck::IV(InputValidation::AP(s));
            }
            InputValidation::BH(s) => {
                if add_new {
                    self.configs.pop();
                }
                return AppConfigCheck::IV(InputValidation::BH(s));
            }
            InputValidation::BP(s) => {
                if add_new {
                    self.configs.pop();
                }
                return AppConfigCheck::IV(InputValidation::BP(s));
            }
        }

        match self.check_app_conflicts(app_index) {
            AppConflicts::NONE => {}
            AppConflicts::CONFLICT((app, con_component)) => {
                if add_new {
                    self.configs.pop();
                }
                return AppConfigCheck::AC(AppConflicts::CONFLICT((app, con_component)));
            }
        }

        let _ = fs::remove_file(&self.configs[app_index].0.config_path);

        #[cfg(target_os = "windows")]
        {
            self.configs[app_index].0.config_path = format!(
                "{}\\VORAppConfigs\\{}.json",
                vor_root().expect("[-] Roaming directory can't be found!"),
                self.configs[app_index].0.config_data.app_name
            );
        }

        #[cfg(target_os = "linux")]
        {
            self.configs[app_index].0.config_path = format!(
                "{}/.vor/VORAppConfigs/{}.json",
                get_user_home_dir(),
                self.configs[app_index].0.config_data.app_name
            );
        }

        fs::write(
            &self.configs[app_index].0.config_path,
            serde_json::to_string(&self.configs[app_index].0.config_data).unwrap(),
        )
        .unwrap();

        return AppConfigCheck::SUCCESS;
    }

    fn check_app_conflicts(&mut self, app_index: usize) -> AppConflicts {
        for i in 0..self.configs.len() {
            if i != app_index {
                if self.configs[i].0.config_data.app_name
                    == self.configs[app_index].0.config_data.app_name
                {
                    return AppConflicts::CONFLICT((
                        self.configs[i].0.config_data.app_name.clone(),
                        "App Name".to_string(),
                    ));
                }
                /*
                if self.configs[i].0.config_data.bind_host == self.configs[app_index].0.config_data.bind_host {
                    return AppConflicts::CONFLICT((self.configs[app_index].0.config_data.app_name.clone(), "Bind Host".to_string()))
                }*/
                /*
                if self.configs[i].0.config_data.bind_port
                    == self.configs[app_index].0.config_data.bind_port
                {
                    return AppConflicts::CONFLICT((
                        self.configs[i].0.config_data.app_name.clone(),
                        "Bind Port".to_string(),
                    ));
                }

                if self.configs[app_index].0.config_data.bind_port
                    == self.vor_router_config.bind_port
                {
                    return AppConflicts::CONFLICT(("VOR".to_string(), "Bind Port".to_string()));
                }*/
                /*
                if self.configs[i].0.config_data.app_host == self.configs[app_index].0.config_data.app_host {
                    return AppConflicts::CONFLICT((self.configs[app_index].0.config_data.app_name.clone(), "App Host".to_string()))
                }*/
                /*
                if self.configs[i].0.config_data.app_port == self.configs[app_index].0.config_data.app_port {
                    return AppConflicts::CONFLICT((self.configs[i].0.config_data.app_name.clone(), "App Port".to_string()))
                }
                */
            }
        }
        return AppConflicts::NONE;
    }

    fn check_app_inputs(&mut self, app_index: usize) -> InputValidation {
        if !check_valid_ipv4(&self.configs[app_index].0.config_data.app_host) {
            return InputValidation::AH(false);
        }

        /*
        if !check_valid_ipv4(&self.configs[app_index].0.config_data.bind_host) {
            return InputValidation::BH(false);
        }*/

        if !check_valid_port(&self.configs[app_index].0.config_data.app_port) {
            return InputValidation::AP(false);
        }

        /*
        if !check_valid_port(&self.configs[app_index].0.config_data.bind_port) {
            return InputValidation::BP(false);
        }*/

        return InputValidation::CLEAN;
    }

    fn add_app(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            if self.adding_new_app {
                ui.horizontal_wrapped(|ui| {
                    ui.label("App Name: ");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.app_name));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("App Host: ");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.app_host));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("App Port: ");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.app_port));
                });
                /*
                ui.horizontal_wrapped(|ui| {
                    ui.label("Bind Host:");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.bind_host));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Bind Port:");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.bind_port));
                });*/

                ui.horizontal_wrapped(|ui| {
                    match &self.new_app_cf_exists_err {
                        AppConfigCheck::AC(ac) => {
                            ui.colored_label(Color32::RED, ac.to_string());
                        },
                        AppConfigCheck::IV(iv) => {
                            ui.colored_label(Color32::RED, iv.to_string());
                        },
                        AppConfigCheck::SUCCESS => {},
                    }
                    //ui.separator();
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(), |ui| {
                            if ui.button(RichText::new("Cancel").color(Color32::RED)).clicked() {
                                self.new_app = None;
                                self.adding_new_app = false;
                                self.new_app_cf_exists_err = AppConfigCheck::SUCCESS;
                            }
                            if ui.button(RichText::new("Add")).clicked() {

                                #[cfg(target_os = "windows")]
                                {
                                    self.new_app.as_mut().unwrap().config_path = format!("{}\\VORAppConfigs\\{}.json", vor_root().expect("[-] Roaming directory can't be found!"), self.new_app.as_ref().unwrap().config_data.app_name);
                                }

                                #[cfg(target_os = "linux")]
                                {
                                    self.new_app.as_mut().unwrap().config_path = format!("{}/.vor/VORAppConfigs/{}.json", get_user_home_dir(), self.new_app.as_ref().unwrap().config_data.app_name);
                                }

                                if !file_exists(&self.new_app.as_ref().unwrap().config_path) {
                                    self.configs.push((self.new_app.as_ref().unwrap().clone(), VORAppStatus::Stopped, AppConfigState::SAVED));

                                    match self.save_app_config(self.configs.len()-1, self.adding_new_app) {
                                        AppConfigCheck::AC(ac) => {
                                            self.new_app_cf_exists_err = AppConfigCheck::AC(ac);
                                            //println!("[!!] Add new -> AC err");
                                        },
                                        AppConfigCheck::IV(iv) => {
                                            self.new_app_cf_exists_err = AppConfigCheck::IV(iv);
                                            //println!("[!!] Add new -> IV err");
                                        },
                                        AppConfigCheck::SUCCESS => {
                                            self.adding_new_app = false;
                                            self.new_app_cf_exists_err = AppConfigCheck::SUCCESS;
                                        }
                                    }
                                } else {//println!("[!] Config conflict!");

                                    /*
                                    if self.vor_router_config.bind_port == self.new_app.as_ref().unwrap().config_data.bind_port {
                                        self.new_app_cf_exists_err = AppConfigCheck::AC(AppConflicts::CONFLICT((self.new_app.as_ref().unwrap().config_data.bind_port.clone(), "VOR bind port conflict".to_string())));
                                    } else {
                                        self.new_app_cf_exists_err = AppConfigCheck::AC(AppConflicts::CONFLICT((self.new_app.as_ref().unwrap().config_data.app_name.clone(), "App Name".to_string())));
                                    }*/
                                }
                            }
                        });
                    });
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label("Add new VOR app");
                    ui.with_layout(Layout::right_to_left(), |ui| {
                        if ui.button(RichText::new("+").color(Color32::GREEN).monospace()).clicked() {
                            self.new_app = Some(VORConfigWrapper {
                                config_path: String::new(),
                                config_data: VORConfig {
                                    app_port: "9100".to_string(),
                                    app_host: "127.0.0.1".to_string(),
                                    //bind_port: "9101".to_string(),
                                    //bind_host: "127.0.0.1".to_string(),
                                    app_name: "New App".to_string(),
                                },
                            });// new_app defaults
                            self.adding_new_app = true;// Being added
                        }// New button
                    });
                });
            }
        });
    }

    fn gui_header(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("VOR");
            ui.add_space(3.);
        });
        ui.separator();
    }

    fn gui_footer(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(5.0);
                ui.add(Hyperlink::from_label_and_url(
                    "VOR",
                    "https://github.com/SutekhVRC/VOR",
                ));
                ui.label(VERSION);
                ui.add(Hyperlink::from_label_and_url(
                    RichText::new("Made by Sutekh")
                        .monospace()
                        .color(Color32::WHITE),
                    "https://github.com/SutekhVRC",
                ));
                ui.add_space(5.0);
            });
        });
    }

    fn list_app_configs(&mut self, ui: &mut egui::Ui) {
        let conf_count = self.configs.len();
        ////print!("[CONF COUNT: {}\r", conf_count);
        for i in 0..conf_count {
            ////println!("[+] Config Length: {}", self.configs.len());
            let check;
            if conf_count == self.configs.len() {
                check = self.configs[i].2.clone();
            } else {
                break;
            }
            ui.group(|ui| {
                match check {

                    AppConfigState::EDIT(ref chk) => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("App Name: ");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.app_name));
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("App Host: ");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.app_host));
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("App Port: ");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.app_port));
                        });
                        /*
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Bind Host:");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.bind_host));
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Bind Port:");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.bind_port));
                        });*/

                        match chk {
                            AppConfigCheck::AC(c) => {
                                ui.horizontal(|ui| {
                                    ui.group(|ui| {
                                        ui.colored_label(Color32::RED, format!("App conflict: {}", c));
                                        /*
                                        ui.label(format!("{}: ", self.configs[i].0.config_data.app_name));
                                        ui.add(Label::new(RichText::new(format!("{}", c)).color(Color32::GOLD)).wrap(true));*/
                                    });
                                });
                            },
                            AppConfigCheck::IV(iv) => {
                                ui.horizontal(|ui| {
                                    ui.group(|ui| {
                                        ui.colored_label(Color32::RED, format!("App invalid input: {}", iv));
                                        /*
                                        ui.label(format!("{}: ", self.configs[i].0.config_data.app_name));
                                        ui.add(Label::new(RichText::new(format!("{}", iv)).color(Color32::GOLD)).wrap(true));*/
                                    });
                                });
                            },
                            AppConfigCheck::SUCCESS => {},// No previous error
                        }

                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(), |ui| {
                                if ui.button(RichText::new("Save")).clicked() {
                                    // Save config / Input val / Val collision
                                    match self.save_app_config(i, false) {
                                        AppConfigCheck::SUCCESS => {
                                            self.configs[i].2 = AppConfigState::SAVED;// Not being edited
                                        },
                                        AppConfigCheck::AC(ac) => {
                                            // Conflicting input errors
                                            //ui.colored_label(Color32::GOLD, format!("App conflict: {}", ac));
                                            self.configs[i].2 = AppConfigState::EDIT(AppConfigCheck::AC(ac));
                                        },
                                        AppConfigCheck::IV(iv) => {
                                            // Input invalid
                                            //ui.colored_label(Color32::GOLD, format!("App invalid input: {}", iv));
                                            self.configs[i].2 = AppConfigState::EDIT(AppConfigCheck::IV(iv));
                                        },
                                    }
                                }
                            });
                        });
                    },
                    AppConfigState::SAVED => {

                        ui.horizontal(|ui| {
                            ui.label(self.configs[i].0.config_data.app_name.as_str());

                            ui.with_layout(Layout::right_to_left(), |ui| {
                                //if !self.running {
                                match &self.configs[i].1 {
                                    VORAppStatus::Running => {
                                        ui.colored_label(Color32::RED, "Locked");
                                    },
                                    VORAppStatus::Stopped | VORAppStatus::Disabled => {
                                        if ui.button(RichText::new("-").color(Color32::RED).monospace()).clicked() {
                                            fs::remove_file(&self.configs[i].0.config_path).unwrap();
                                            self.configs.remove(i);
                                            return;
                                        }
                                        if ui.button(RichText::new("Edit")).clicked() {
                                            self.configs[i].2 = AppConfigState::EDIT(AppConfigCheck::SUCCESS);// Being edited
                                        }
                                        if let VORAppStatus::Disabled = self.configs[i].1 {
                                            if ui.button(RichText::new("Enable")).clicked() {
                                                self.configs[i].1 = VORAppStatus::Stopped;
                                            }
                                        } else {
                                            if ui.button(RichText::new("Disable")).clicked() {
                                                self.configs[i].1 = VORAppStatus::Disabled;
                                            }
                                        }
                                    },
                                    VORAppStatus::AppError(_e) => {
                                        ui.colored_label(Color32::RED, "Error");
                                    }
                                }
                            });
                        });
                    },
                }
            });
        } // For list
    }

    fn pf_buttons(&mut self, ui: &mut egui::Ui) {
        if !self.pf.enabled {
            return;
        }
        ui.checkbox(&mut self.pf.filter_bad_packets, "Filter bad packets");
        if !self.pf.bl_enabled {
            ui.checkbox(&mut self.pf.wl_enabled, "Whitelisting");
        }
        if !self.pf.wl_enabled {
            ui.checkbox(&mut self.pf.bl_enabled, "Blacklisting");
        }
    }

    fn save_pf_config(&mut self) {
        #[cfg(target_os = "windows")]
        {
            fs::write(
                format!(
                    "{}\\VOR_PF.json",
                    vor_root().expect("[-] Roaming directory can't be found!")
                ),
                serde_json::to_string(&self.pf).unwrap(),
            )
            .unwrap();
        }

        #[cfg(target_os = "linux")]
        {
            fs::write(
                format!("{}/.vor/VOR_PF.json", get_user_home_dir()),
                serde_json::to_string(&self.pf).unwrap(),
            )
            .unwrap();
        }
    }

    fn pf_whitelist(&mut self, ui: &mut egui::Ui) {
        if self.pf.wl_enabled {
            let wl_add_count = self.pf.address_wl.len();

            if wl_add_count >= 1 {
                for i in 0..wl_add_count {
                    let mut removed = false;
                    if !self.pf.address_wl[i].1 {
                        ui.horizontal(|ui| {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new(&self.pf.address_wl[i].0).monospace());
                                ui.with_layout(Layout::right_to_left(), |ui| {
                                    if ui
                                        .button(RichText::new("-").monospace().color(Color32::RED))
                                        .clicked()
                                    {
                                        self.pf.address_wl.remove(i);
                                        removed = true;
                                    }

                                    if ui.button(RichText::new("Edit").monospace()).clicked() {
                                        self.pf.address_wl[i].1 = true;
                                    }
                                });
                            });
                        });

                        // Restart loop to not crash
                        if removed {break;}
                    } else {
                        //edit entry
                        ui.horizontal_wrapped(|ui| {
                            ui.group(|ui| {
                                ui.with_layout(Layout::right_to_left(), |ui| {
                                    if ui.button("Save").clicked() {
                                        // Save to file
                                        self.pf.address_wl[i].1 = false;
                                        self.save_pf_config();
                                    }
                                    ui.text_edit_singleline(&mut self.pf.address_wl[i].0);
                                });
                            });
                        });
                    }
                }
            }
        }
    }

    fn add_pf_wl(&mut self, ui: &mut egui::Ui) {
        // if not adding
        if !self.pf_wl_new.1 {
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    ui.label("New filter");
                    ui.with_layout(Layout::right_to_left(), |ui| {
                        if ui
                            .button(RichText::new("+").color(Color32::GREEN))
                            .clicked()
                        {
                            //self.pf.wl_editing = true;
                            self.save_pf_config();
                            self.pf_wl_new.1 = true;
                        }
                    });
                });
            });
        } else {
            // if adding

            ui.horizontal_wrapped(|ui| {
                ui.label("Filter: ");
                ui.text_edit_singleline(&mut self.pf_wl_new.0);
            });
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(), |ui| {
                    if ui.button("Cancel").clicked() {
                        self.pf_wl_new.1 = false;
                        self.pf_wl_new.0.clear();
                    }
                    if ui.button("Add filter").clicked() {
                        self.pf_wl_new.1 = false;
                        self.pf.address_wl.push(self.pf_wl_new.clone());
                        self.pf_wl_new.0.clear();
                        self.save_pf_config();
                    }
                });
            });
        }
    }

    fn pf_blacklist(&mut self, ui: &mut egui::Ui) {
        if self.pf.bl_enabled {
            let bl_add_count = self.pf.address_bl.len();

            if bl_add_count >= 1 {
                for i in 0..bl_add_count {
                    let mut removed = false;
                    if !self.pf.address_bl[i].1 {
                        ui.horizontal(|ui| {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new(&self.pf.address_bl[i].0).monospace());
                                
                                ui.with_layout(Layout::right_to_left(), |ui| {
                                    if ui
                                        .button(RichText::new("-").monospace().color(Color32::RED))
                                        .clicked()
                                    {
                                        self.pf.address_bl.remove(i);
                                        removed = true;
                                    }

                                    if ui.button(RichText::new("Edit").monospace()).clicked() {
                                        self.pf.address_bl[i].1 = true;
                                    }
                                });
                            });
                        });

                        // Restart loop to not crash
                        if removed {break;}
                    } else {
                        //edit entry
                        ui.horizontal_wrapped(|ui| {
                            ui.group(|ui| {
                                ui.with_layout(Layout::right_to_left(), |ui| {
                                    if ui.button("Save").clicked() {
                                        // Save to file
                                        self.pf.address_bl[i].1 = false;
                                        self.save_pf_config();
                                    }
                                    ui.text_edit_singleline(&mut self.pf.address_bl[i].0);
                                });
                            });
                        });
                    }
                }
            }
        }
    }

    fn add_pf_bl(&mut self, ui: &mut egui::Ui) {
        // if not adding
        if !self.pf_bl_new.1 {
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    ui.label("New filter");
                    ui.with_layout(Layout::right_to_left(), |ui| {
                        if ui
                            .button(RichText::new("+").color(Color32::GREEN))
                            .clicked()
                        {
                            //self.pf.wl_editing = true;
                            self.pf_bl_new.1 = true;
                        }
                    });
                });
            });
        } else {
            // if adding

            ui.horizontal_wrapped(|ui| {
                ui.label("Entry: ");
                ui.text_edit_singleline(&mut self.pf_bl_new.0);
            });
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(), |ui| {
                    if ui.button("Cancel").clicked() {
                        self.pf_bl_new.1 = false;
                        self.pf_bl_new.0.clear();
                    }
                    if ui.button("Add filter").clicked() {
                        self.pf_bl_new.1 = false;
                        self.pf.address_bl.push(self.pf_bl_new.clone());
                        self.pf_bl_new.0.clear();
                    }
                });
            });
        }
    }
} // impl VORGUI

impl App for VORGUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.set_tab(&ctx);
        CentralPanel::default().show(ctx, |ui| {
            //ctx.request_repaint();

            self.gui_header(ui);

            match self.tab {
                VORGUITab::Main => {
                    ui.horizontal_wrapped(|ui| {
                        ui.add(egui::Label::new("VOR Main"));
                        self.router_exec_button(ui, &ctx);
                    });

                    ui.separator();

                    self.status(ui);
                    ui.add_space(60.);
                }
                VORGUITab::Apps => {
                    ui.add(egui::Label::new("VOR App Configs"));
                    ui.separator();
                    ScrollArea::new([false, true]).show(ui, |ui| {
                        self.list_app_configs(ui);
                        self.add_app(ui);
                        ui.add_space(60.);
                    });
                }
                VORGUITab::Firewall => {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.pf.enabled, "OSC Packet Filter");
                        ui.with_layout(Layout::right_to_left(), |ui| {
                            if ui.button("Save").clicked() {
                                self.save_pf_config();
                            }
                        });
                    });

                    ui.separator();
                    self.pf_buttons(ui);
                    if self.pf.enabled {
                        ui.separator();

                        if self.pf.wl_enabled {
                            ui.label(RichText::new("Whitelist"));
                            ScrollArea::new([false, true]).show(ui, |ui| {
                                self.pf_whitelist(ui);
                                self.add_pf_wl(ui);
                                ui.add_space(60.);
                            });
                        } else if self.pf.bl_enabled {
                            ui.label(RichText::new("Blacklist"));
                            ScrollArea::new([false, true]).show(ui, |ui| {
                                self.pf_blacklist(ui);
                                self.add_pf_bl(ui);
                                ui.add_space(60.);
                            });
                        }
                    }
                }
                VORGUITab::Config => {
                    ui.horizontal_wrapped(|ui| {
                        ui.add(egui::Label::new("VOR Config"));
                        ui.with_layout(Layout::right_to_left(), |ui| {
                            if ui.button(RichText::new("Save")).clicked() {
                                self.save_vor_config();
                            }
                        });
                    });
                    ui.separator();
                    self.list_vor_config(ui);
                }
            }

            // Debug window
            if self.route_debug.is_some() {
                self.debug_status_refresh();
                self.debug_window(ui, ctx);
            } else {
            }
        });
        self.gui_footer(&ctx);
    }
}

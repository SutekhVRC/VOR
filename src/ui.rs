use eframe::egui::{ScrollArea, Layout, RichText, TopBottomPanel, Hyperlink, Context, Label};
use eframe::epaint::Color32;
use std::sync::mpsc::{self, Sender, Receiver};
use eframe::{epi::{App}, egui::{self, CentralPanel}};
use std::{fs, thread};
use crate::{
    routing::{
        RouterConfig,
        RouterMsg,
        route_main,
    },
    VORConfigWrapper,
    VORAppStatus,
    AppConfigState,
    VORAppIdentifier,
    config_construct,
    VORConfig,
    get_user_home_dir,
    AppConfigCheck,
    InputValidation,
    AppConflicts,
    check_valid_ipv4,
    check_valid_port,
    file_exists,
};

pub struct VORGUI {
    configs: Vec<(VORConfigWrapper, VORAppStatus, AppConfigState)>,
    running: bool,
    tab: VORGUITab,
    router_channel: Option<Sender<RouterMsg>>,
    vor_router_config: RouterConfig,
    adding_new_app: bool,
    new_app: Option<VORConfigWrapper>,
    new_app_cf_exists_err: bool,
    router_msg_recvr: Option<Receiver<VORAppIdentifier>>,
}

pub enum VORGUITab {
    Main,
    Apps,
    Config,
    Firewall,
}

impl VORGUI {

    pub fn new(configs: Vec<(VORConfigWrapper, VORAppStatus, AppConfigState)>, vor_router_config: RouterConfig) -> Self {
        VORGUI {
            configs,
            running: false,
            tab: VORGUITab::Main,
            router_channel: None,
            vor_router_config,
            adding_new_app: false,
            new_app: None,
            new_app_cf_exists_err: false,
            router_msg_recvr: None,
        }
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
    
                        
                        if ui.button(RichText::new("Firewall").monospace()).clicked() {
                            self.tab = VORGUITab::Firewall;
                        }
                        ui.separator();
    
                        if ui.button(RichText::new("Config").monospace()).clicked() {
                            self.tab = VORGUITab::Config;
                        }

                        ui.separator();
                        ui.with_layout(Layout::right_to_left(), |ui| {
                            if self.running {
                                ui.label(RichText::new("Routing").color(Color32::GREEN));
                            } else {
                                ui.label(RichText::new("Stopped").color(Color32::RED));
                            }
                        });
                        //ui.separator();
                    });
                    //ui.separator();
                });
            });
        });
    }

    fn status_refresh(&mut self) {
        let status = match self.router_msg_recvr.as_ref(){
            Some(recvr) => {
                match recvr.try_recv() {
                    Ok(status) => status,
                    Err(_e) => {return;},
                }
            },
            None => return,
        };
        self.configs[status.index as usize].1 = status.status;
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
                        VORAppStatus::Running => {},
                        VORAppStatus::Stopped => {status_color = Color32::RED},
                        VORAppStatus::AppError(_) => {status_color = Color32::GOLD},
                    }
                    ui.horizontal(|ui| {
                        ui.group(|ui| {
                        ui.label(format!("{}", self.configs[i].0.config_data.app_name));
                            ui.with_layout(Layout::right_to_left(), |ui| {
                                ui.separator();
                                ui.add(Label::new(RichText::new(format!("{}", self.configs[i].1)).color(status_color)).wrap(true));
                            });
                        });
                    });
                }
            }
        });
    }

    fn list_vor_config(&mut self, ui: &mut egui::Ui) {
        // UI for VOR config
        ui.label("Bind Host: ");ui.add(egui::TextEdit::singleline(&mut self.vor_router_config.bind_host));
        ui.label("Bind Port: ");ui.add(egui::TextEdit::singleline(&mut self.vor_router_config.bind_port));
        ui.label("VRChat Host: ");ui.add(egui::TextEdit::singleline(&mut self.vor_router_config.vrc_host));
        ui.label("VRChat Port: ");ui.add(egui::TextEdit::singleline(&mut self.vor_router_config.vrc_port));
        ui.label("VOR Buffer Size: ");ui.add(egui::TextEdit::singleline(&mut self.vor_router_config.vor_buffer_size));

    }

    fn router_exec_button(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(), |ui| {
                if self.running {
                    //ui.group(|ui| {
                        if ui.button("Stop").clicked() {
                            if self.running {
                                self.stop_router();
                                ctx.request_repaint();
                            }
                        }
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
                        }
                    //});
                } else {
                    //ui.group(|ui| {
                        if ui.button(RichText::new("Start").color(Color32::GREEN)).clicked() {
                            
                            if !self.running {
                                self.start_router();
                                ctx.request_repaint();
                            }
                        }
                    //});
                }
            });
            ui.separator();
        });
    }

    fn start_router(&mut self) {
        // Create main router thread - 1 channel store TX in GUI object
            // router thread recv msgs from GUI thread and controls child threads each with their own channel to comm with router thread
        // Generate / Start OSC threads here
        let confs: Vec<VORConfig> = self.configs.iter().map(|c| c.0.config_data.clone()).collect();

        let (router_tx, router_rx): (Sender<RouterMsg>, Receiver<RouterMsg>) = mpsc::channel();
        let (app_stat_tx, app_stat_rx): (Sender<VORAppIdentifier>, Receiver<VORAppIdentifier>) = mpsc::channel();
        self.router_channel = Some(router_tx);
        self.router_msg_recvr = Some(app_stat_rx);

        let bind_target = format!("{}:{}", self.vor_router_config.bind_host, self.vor_router_config.bind_port);
        let vor_buf_size = match self.vor_router_config.vor_buffer_size.parse::<usize>() {
            Ok(s) => s,
            Err(_) => {
                self.router_channel = None;
                //self.router_msg_recvr = None;
                return;
            }
        };
        thread::spawn(move || {
            route_main(bind_target, router_rx, app_stat_tx, confs, vor_buf_size);
        });

        self.running = true;
    }

    fn stop_router(&mut self) {
        // Send shutdown signal to OSC threads here
        self.router_channel.take().unwrap().send(RouterMsg::ShutdownAll).unwrap();
        //self.router_msg_recvr = None;
        self.running = false;
    }

    fn save_vor_config(&mut self) {
        fs::write(format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\VORConfig.json", get_user_home_dir()), serde_json::to_string(&self.vor_router_config).unwrap()).unwrap();
    }

    fn save_app_config(&mut self, app_index: usize) -> AppConfigCheck {

        match self.check_app_inputs(app_index) {
            InputValidation::CLEAN => {},
            InputValidation::AH(s) => {
                return AppConfigCheck::IV(InputValidation::AH(s));
            },
            InputValidation::AP(s) => {
                return AppConfigCheck::IV(InputValidation::AP(s));
            },
            InputValidation::BH(s) => {
                return AppConfigCheck::IV(InputValidation::BH(s));
            },
            InputValidation::BP(s) => {
                return AppConfigCheck::IV(InputValidation::BP(s));
            },
        }

        match self.check_app_conflicts(app_index) {
            AppConflicts::NONE => {},
            AppConflicts::CONFLICT((app, con_component)) => {
                return AppConfigCheck::AC(AppConflicts::CONFLICT((app, con_component)));
            }
        }

        let _ = fs::remove_file(&self.configs[app_index].0.config_path);
        self.configs[app_index].0.config_path = format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\VorAppConfigs\\{}.json", get_user_home_dir(), self.configs[app_index].0.config_data.app_name);
        fs::write(&self.configs[app_index].0.config_path, serde_json::to_string(&self.configs[app_index].0.config_data).unwrap()).unwrap();

        return AppConfigCheck::SUCCESS;
    }

    fn check_app_conflicts(&mut self, app_index: usize) -> AppConflicts {

        for i in 0..self.configs.len() {
            if i != app_index {
                
                if self.configs[i].0.config_data.app_name == self.configs[app_index].0.config_data.app_name {
                    return AppConflicts::CONFLICT((self.configs[i].0.config_data.app_name.clone(), "App Name".to_string()))
                }
                /*
                if self.configs[i].0.config_data.bind_host == self.configs[app_index].0.config_data.bind_host {
                    return AppConflicts::CONFLICT((self.configs[app_index].0.config_data.app_name.clone(), "Bind Host".to_string()))
                }*/
                if self.configs[i].0.config_data.bind_port == self.configs[app_index].0.config_data.bind_port {
                    return AppConflicts::CONFLICT((self.configs[i].0.config_data.app_name.clone(), "Bind Port".to_string()))
                }

                if self.configs[app_index].0.config_data.bind_port == self.vor_router_config.bind_port {
                    return AppConflicts::CONFLICT(("VOR".to_string(), "Bind Port".to_string()))
                }
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
        
        if !check_valid_ipv4(&self.configs[app_index].0.config_data.bind_host) {
            return InputValidation::BH(false);
        }
        
        if !check_valid_port(&self.configs[app_index].0.config_data.app_port) {
            return InputValidation::AP(false);
        }
        
        if !check_valid_port(&self.configs[app_index].0.config_data.bind_port) {
            return InputValidation::BP(false);
        }

        return InputValidation::CLEAN;
    }

    fn add_app(&mut self, ui: &mut egui::Ui) {

        ui.group(|ui| {
            if self.adding_new_app {
                ui.label("App Name");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.app_name));
                ui.label("App Host");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.app_host));
                ui.label("App Port");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.app_port));
                ui.label("Bind Host");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.bind_host));
                ui.label("Bind Port");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.bind_port));

                ui.horizontal_wrapped(|ui| {
                    if self.new_app_cf_exists_err {
                        
                        ui.colored_label(Color32::RED, "App config name already being used.. Choose different app name.");
                        ui.separator();
                    }
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(), |ui| {
                            if ui.button(RichText::new("Cancel").color(Color32::RED)).clicked() {
                                self.new_app = None;
                                self.adding_new_app = false;
                                self.new_app_cf_exists_err = false;
                            }
                            if ui.button(RichText::new("Add")).clicked() {
                                self.new_app.as_mut().unwrap().config_path = format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\VORAppConfigs\\{}.json", get_user_home_dir(), self.new_app.as_ref().unwrap().config_data.app_name);
                                if !file_exists(&self.new_app.as_ref().unwrap().config_path) {
                                    self.configs.push((self.new_app.take().unwrap(), VORAppStatus::Stopped, AppConfigState::SAVED));
                                    self.save_app_config(self.configs.len()-1);
                                    self.adding_new_app = false;
                                    self.new_app_cf_exists_err = false;
                                } else {
                                    self.new_app_cf_exists_err = true;
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
                                    bind_port: "9101".to_string(),
                                    bind_host: "127.0.0.1".to_string(),
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
        TopBottomPanel::bottom("footer").show(ctx, |ui|{
            ui.vertical_centered(|ui| {
                ui.add_space(5.0);
                ui.add(Hyperlink::from_label_and_url("VOR","https://github.com/SutekhVRC/VOR"));
                ui.label("0.1.0-beta");
                ui.add(Hyperlink::from_label_and_url(RichText::new("Made by Sutekh").monospace().color(Color32::WHITE),"https://github.com/SutekhVRC"));
                ui.add_space(5.0);
            });
        });
    }

    fn list_app_configs(&mut self, ui: &mut egui::Ui) {
        let conf_count = self.configs.len();
        for i in 0..conf_count {
            //println!("[+] Config Length: {}", self.configs.len());
            let check;
            if conf_count == self.configs.len() {
                check = self.configs[i].2.clone();
            } else {
                break;
            }
            ui.group(|ui| {
                match check {

                    AppConfigState::EDIT(ref chk) => {
                        ui.label("App Name");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.app_name));
                        ui.label("App Host");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.app_host));
                        ui.label("App Port");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.app_port));
                        ui.label("Bind Host");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.bind_host));
                        ui.label("Bind Port");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.bind_port));
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
                                ui.group(|ui| {
                                    if ui.button(RichText::new("Save")).clicked() {
                                        // Save config / Input val / Val collision
                                        match self.save_app_config(i) {
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
                        });
                    },
                    AppConfigState::SAVED => {
                        
                            
                                ui.horizontal(|ui| {
                                    ui.label(self.configs[i].0.config_data.app_name.as_str());
            
                                    ui.with_layout(Layout::right_to_left(), |ui| {
                                        if !self.running {
                                            if ui.button(RichText::new("-").color(Color32::RED).monospace()).clicked() {
                                                fs::remove_file(&self.configs[i].0.config_path).unwrap();
                                                self.configs.remove(i);
                                            }
                                            if ui.button(RichText::new("Edit")).clicked() {
                                                self.configs[i].2 = AppConfigState::EDIT(AppConfigCheck::SUCCESS);// Being edited
                                            }
                                        } else {
                                            ui.colored_label(Color32::RED, "Locked");
                                        }
                                    });
                                });
                    },
                }
            });
        }// For list
    }
}// impl VORGUI


impl App for VORGUI {
    fn setup(&mut self, _ctx: &egui::Context, _frame: &eframe::epi::Frame, _storage: Option<&dyn eframe::epi::Storage>) {
        // Read config values

        // Set fonts etc.
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &eframe::epi::Frame) {
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


                },
                VORGUITab::Apps => {
                    ui.add(egui::Label::new("VOR App Configs"));
                    ui.separator();
                    ScrollArea::new([false, true]).show(ui, |ui| {
                        self.list_app_configs(ui);
                        self.add_app(ui);
                        ui.add_space(60.);
                    });
                },
                VORGUITab::Firewall => {
                    ui.add(egui::Label::new("OSC Firewall"));
                    ui.separator();
                    ui.colored_label(Color32::RED, "Not implemented yet.");
                },
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
                },
            }
        });
        self.gui_footer(&ctx);
    }

    fn name(&self) -> &str {
        "VRChat OSC Router"
    }
}
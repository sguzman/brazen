use super::*;
use crate::engine::InputEvent;

impl BrazenApp {
    pub(super) fn forward_input_events(&mut self, ctx: &eframe::egui::Context) {
        let input = ctx.input(|input| input.clone());
        let focused = if input.raw.focused {
            FocusState::Focused
        } else {
            FocusState::Unfocused
        };
        self.engine.set_focus(focused);
        let input_logging = self.config.engine.input_logging;
        let suppress_engine_input = self.command_palette_open;

        if let Some(minimized) = input.raw.viewport().minimized {
            if minimized && !self.shell_state.was_minimized {
                self.engine.suspend();
                self.shell_state.was_minimized = true;
            } else if !minimized && self.shell_state.was_minimized {
                self.engine.resume();
                self.shell_state.was_minimized = false;
            }
        }

        for event in input.raw.events {
            match event {
                eframe::egui::Event::PointerMoved(pos) => {
                    self.last_pointer_pos = Some(pos);
                    if let Some(local) =
                        self.map_pointer_to_viewport(ctx, pos, self.pointer_captured)
                    {
                        self.last_pointer_local = Some(local);
                        if !self.pointer_inside {
                            self.pointer_inside = true;
                            self.engine.handle_input(InputEvent::PointerEnter {
                                x: local.x,
                                y: local.y,
                            });
                        }
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                x = local.x,
                                y = local.y,
                                "pointer moved"
                            );
                        }
                        self.engine.handle_input(InputEvent::PointerMove {
                            x: local.x,
                            y: local.y,
                        });
                    } else {
                        self.last_pointer_local = None;
                        if self.pointer_inside && !self.pointer_captured {
                            self.pointer_inside = false;
                            self.engine.handle_input(InputEvent::PointerLeave);
                        }
                    }
                }
                eframe::egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    ..
                } => {
                    self.last_pointer_pos = Some(pos);
                    let button_id = match button {
                        eframe::egui::PointerButton::Primary => 0,
                        eframe::egui::PointerButton::Secondary => 1,
                        eframe::egui::PointerButton::Middle => 2,
                        eframe::egui::PointerButton::Extra1 => 3,
                        eframe::egui::PointerButton::Extra2 => 4,
                    };
                    let allow_outside = self.pointer_captured || pressed;
                    if let Some(local) = self.map_pointer_to_viewport(ctx, pos, allow_outside) {
                        self.last_pointer_local = Some(local);
                        if !self.pointer_inside {
                            self.pointer_inside = true;
                            self.engine.handle_input(InputEvent::PointerEnter {
                                x: local.x,
                                y: local.y,
                            });
                        }
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                button = button_id,
                                pressed,
                                x = local.x,
                                y = local.y,
                                "pointer button"
                            );
                        }
                        if pressed {
                            let click_count = self.update_click_count(button, pos);
                            if button == eframe::egui::PointerButton::Secondary {
                                self.shell_state.pending_context_menu = Some((pos.x, pos.y));
                                self.shell_state.record_event(format!(
                                    "context menu requested: {:.0},{:.0}",
                                    pos.x, pos.y
                                ));
                            } else {
                                self.shell_state.pending_context_menu = None;
                            }
                            self.pointer_captured = matches!(
                                button,
                                eframe::egui::PointerButton::Primary
                                    | eframe::egui::PointerButton::Middle
                            );
                            self.engine.handle_input(InputEvent::PointerDown {
                                button: button_id,
                                click_count,
                            });
                        } else {
                            self.pointer_captured = false;
                            self.engine
                                .handle_input(InputEvent::PointerUp { button: button_id });
                        }
                    } else if !pressed {
                        self.pointer_captured = false;
                        if self.pointer_inside {
                            self.pointer_inside = false;
                            self.engine.handle_input(InputEvent::PointerLeave);
                        }
                    }
                }
                eframe::egui::Event::MouseWheel { delta, unit, .. } => {
                    if let Some(pos) = input.pointer.latest_pos().or(self.last_pointer_pos)
                        && let Some(local) =
                            self.map_pointer_to_viewport(ctx, pos, self.pointer_captured)
                    {
                        self.last_pointer_local = Some(local);
                        self.engine.handle_input(InputEvent::PointerMove {
                            x: local.x,
                            y: local.y,
                        });
                    }
                    let modifiers = input.modifiers;
                    let axis = if delta.y.abs() >= delta.x.abs() {
                        delta.y
                    } else {
                        delta.x
                    };
                    if modifiers.ctrl || modifiers.command {
                        let steps = if axis.abs() < 0.1 {
                            0
                        } else {
                            axis.signum() as i32
                        };
                        if steps != 0 {
                            self.apply_zoom_steps(steps, "wheel");
                        }
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                axis,
                                steps,
                                "ctrl wheel zoom"
                            );
                        }
                        continue;
                    }
                    let mut delta_x = delta.x;
                    let mut delta_y = delta.y;
                    if modifiers.shift {
                        delta_x = if delta.x.abs() > 0.0 {
                            delta.x
                        } else {
                            delta.y
                        };
                        delta_y = 0.0;
                    }
                    let scale = match unit {
                        eframe::egui::MouseWheelUnit::Line => 24.0,
                        eframe::egui::MouseWheelUnit::Point => 1.0,
                        eframe::egui::MouseWheelUnit::Page => 240.0,
                    };
                    delta_x *= scale;
                    delta_y *= scale;
                    if input_logging {
                        tracing::trace!(
                            target: "brazen::input",
                            delta_x,
                            delta_y,
                            unit = ?unit,
                            "scroll wheel"
                        );
                    }
                    self.engine
                        .handle_input(InputEvent::Scroll { delta_x, delta_y });
                }
                eframe::egui::Event::Zoom(delta) => {
                    if (delta - 1.0).abs() > f32::EPSILON {
                        self.apply_zoom_factor(delta, "pinch");
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                delta,
                                "pinch zoom"
                            );
                        }
                    }
                }
                eframe::egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    repeat,
                    ..
                } => {
                    let is_command = modifiers.ctrl || modifiers.command;
                    let mut handled_shortcut = false;
                    if pressed && is_command {
                        match key {
                            eframe::egui::Key::C | eframe::egui::Key::X => {
                                self.engine
                                    .handle_clipboard(crate::engine::ClipboardRequest::Read);
                                self.shell_state
                                    .record_event(format!("shortcut {:?} => copy", key));
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::A => {
                                self.shell_state.record_event("shortcut: select all");
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::F => {
                                self.shell_state.find_panel_open = true;
                                self.shell_state.record_event("shortcut: find");
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::K => {
                                self.open_command_palette();
                                self.shell_state.record_event("shortcut: command palette");
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::L => {
                                self.address_bar_focus_pending = true;
                                self.shell_state.record_event("shortcut: focus address bar");
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::T => {
                                self.apply_palette_command(PaletteCommand::NewTab);
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::W => {
                                self.apply_palette_command(PaletteCommand::CloseTab);
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::R => {
                                self.apply_palette_command(PaletteCommand::Reload);
                                handled_shortcut = true;
                            }
                            _ => {}
                        }
                    }
                    let zoom_shortcut = pressed
                        && is_command
                        && matches!(
                            key,
                            eframe::egui::Key::Plus
                                | eframe::egui::Key::Equals
                                | eframe::egui::Key::Minus
                                | eframe::egui::Key::Num0
                        );
                    if zoom_shortcut {
                        match key {
                            eframe::egui::Key::Plus | eframe::egui::Key::Equals => {
                                self.apply_zoom_steps(1, "shortcut");
                            }
                            eframe::egui::Key::Minus => {
                                self.apply_zoom_steps(-1, "shortcut");
                            }
                            eframe::egui::Key::Num0 => {
                                self.set_active_tab_zoom(self.config.engine.zoom_default, "reset");
                            }
                            _ => {}
                        }
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                key = ?key,
                                "zoom shortcut"
                            );
                        }
                        continue;
                    }
                    if handled_shortcut {
                        continue;
                    }
                    let key_name = format!("{key:?}");
                    let modifiers = crate::engine::KeyModifiers {
                        alt: modifiers.alt,
                        ctrl: modifiers.ctrl,
                        shift: modifiers.shift,
                        command: modifiers.command,
                    };
                    if !suppress_engine_input {
                        if pressed {
                            self.engine.handle_input(InputEvent::KeyDown {
                                key: key_name,
                                modifiers,
                                repeat,
                            });
                        } else {
                            self.engine.handle_input(InputEvent::KeyUp {
                                key: key_name,
                                modifiers,
                            });
                        }
                    }
                }
                eframe::egui::Event::Text(text) => {
                    if suppress_engine_input {
                        continue;
                    }
                    if input_logging {
                        tracing::trace!(
                            target: "brazen::input",
                            text = %text,
                            "text input"
                        );
                    }
                    self.engine.handle_input(InputEvent::TextInput { text });
                }
                eframe::egui::Event::Ime(ime) => match ime {
                    eframe::egui::ImeEvent::Enabled => {
                        if input_logging {
                            tracing::trace!(target: "brazen::input", "ime enabled");
                        }
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionStart);
                    }
                    eframe::egui::ImeEvent::Preedit(text) => {
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                text = %text,
                                "ime preedit"
                            );
                        }
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionUpdate { text });
                    }
                    eframe::egui::ImeEvent::Commit(text) => {
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                text = %text,
                                "ime commit"
                            );
                        }
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionEnd { text });
                    }
                    eframe::egui::ImeEvent::Disabled => {
                        if input_logging {
                            tracing::trace!(target: "brazen::input", "ime disabled");
                        }
                        self.engine.handle_ime(crate::engine::ImeEvent::Dismissed);
                    }
                },
                eframe::egui::Event::Copy | eframe::egui::Event::Cut => {
                    self.engine
                        .handle_clipboard(crate::engine::ClipboardRequest::Read);
                }
                eframe::egui::Event::Paste(text) => {
                    self.engine
                        .handle_clipboard(crate::engine::ClipboardRequest::Write(text));
                }
                _ => {}
            }
        }

        if !input.raw.dropped_files.is_empty() {
            for file in input.raw.dropped_files {
                let mut target = None;
                if let Some(path) = file.path {
                    if let Ok(url) = url::Url::from_file_path(&path) {
                        target = Some(url.to_string());
                    } else {
                        target = Some(path.to_string_lossy().to_string());
                    }
                } else if file.name.starts_with("http://") || file.name.starts_with("https://") {
                    target = Some(file.name.clone());
                }
                if let Some(target) = target {
                    self.shell_state.address_bar_input = target.clone();
                    let _ = dispatch_command(
                        &mut self.shell_state,
                        self.engine.as_mut(),
                        AppCommand::NavigateTo(target.clone()),
                    );
                    self.shell_state
                        .record_event(format!("dropped file/url: {target}"));
                    break;
                }
            }
        }
    }
}

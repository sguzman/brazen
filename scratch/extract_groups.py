#!/usr/bin/env python3
"""Extract function groups from mod.rs into dedicated submodule files."""

import re, sys

with open('src/app/mod.rs', 'r') as f:
    content = f.read()

def extract_fn(content, fn_signature):
    """Find and extract a top-level impl method starting with fn_signature."""
    idx = content.find(f'    {fn_signature}')
    if idx == -1:
        return content, None
    brace_count = 0
    end_idx = -1
    for i in range(idx, len(content)):
        if content[i] == '{':
            brace_count += 1
        elif content[i] == '}':
            brace_count -= 1
            if brace_count == 0:
                end_idx = i + 1
                break
    if end_idx == -1:
        print(f"WARNING: could not find end of {fn_signature}", file=sys.stderr)
        return content, None
    fn_body = content[idx:end_idx]
    # consume any trailing newlines
    while end_idx < len(content) and content[end_idx] == '\n':
        end_idx += 1
    remaining = content[:idx] + content[end_idx:]
    return remaining, fn_body

def make_pub_super(fn_body):
    """Make the top-level fn keyword pub(super) if not already public."""
    return re.sub(r'^(    )(fn )', r'\1pub(super) \2', fn_body, count=1)

groups = {
    'navigation.rs': [
        'fn handle_navigation(&mut self)',
        'fn map_pointer_to_viewport(',
        'fn sync_active_tab_from_session(&mut self)',
        'fn apply_new_window_policy(&mut self)',
        'fn apply_cursor_icon(&self, ctx: &eframe::egui::Context)',
    ],
    'zoom.rs': [
        'fn clamp_zoom(&self, zoom: f32)',
        'fn set_active_tab_zoom(&mut self, zoom: f32, reason: &str)',
        'fn apply_zoom_steps(&mut self, steps: i32, reason: &str)',
        'fn apply_zoom_factor(&mut self, factor: f32, reason: &str)',
        'fn update_click_count(',
    ],
    'recovery.rs': [
        'fn write_crash_dump(&mut self, reason: &str)',
        'fn restart_engine(&mut self)',
        'fn schedule_restart(&mut self)',
        'fn handle_crash_recovery(&mut self)',
    ],
    'workspace.rs': [
        'fn load_workspace_layout(path: &PathBuf)',
        'fn save_workspace_layout(&self)',
        'fn apply_layout_preset(&mut self, preset: LayoutPreset)',
        'fn palette_entries()',
        'fn open_command_palette(&mut self)',
        'fn apply_palette_command(&mut self, action: PaletteCommand)',
        'fn apply_ui_settings(&self, ctx: &eframe::egui::Context)',
    ],
}

for filename, sigs in groups.items():
    extracted = []
    for sig in sigs:
        content, fn_body = extract_fn(content, sig)
        if fn_body:
            extracted.append(make_pub_super(fn_body))
        else:
            print(f"WARNING: {sig} not found", file=sys.stderr)
    
    if extracted:
        out_path = f'src/app/{filename}'
        with open(out_path, 'w') as f:
            f.write("use super::*;\n\nimpl BrazenApp {\n")
            f.write("\n\n".join(extracted))
            f.write("\n}\n")
        print(f"Wrote {out_path} ({len(extracted)} functions)")

with open('src/app/mod.rs', 'w') as f:
    f.write(content)

print(f"mod.rs now has {len(content.splitlines())} lines")

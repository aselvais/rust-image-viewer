import sys

file_path = "src/explorer.rs"
with open(file_path, "r") as f:
    lines = f.readlines()

# Lines are 0-indexed in Python
# 112 -> 111
# 113 -> 112
# Original lines 112 and 113:
# 112 |                                     let mut child_ui = ui.child_ui(rect, ui.layer_id());
# 113 |                                     child_ui.add(egui::Image::from_texture(&tex).shrink_to_fit());

# We want to replace them with:
# 112 |                                     ui.allocate_ui(rect.size(), |ui| {
# 113 |                                         ui.add(egui::Image::from_texture(&tex).shrink_to_fit());
# 114 |                                     });

new_lines = lines[:111] + [
    "                                     ui.allocate_ui(rect.size(), |ui| {\n",
    "                                         ui.add(egui::Image::from_texture(&tex).shrink_to_fit());\n",
    "                                     });\n"
] + lines[113:]

with open(file_path, "w") as f:
    f.writelines(new_lines)
print("Successfully patched src/explorer.rs")

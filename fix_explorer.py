import sys
import re

file_path = 'src/explorer.rs'

with open(file_path, 'r') as f:
    content = f.read()

# The pattern to search for
old_pattern = r"""                        ui\.vertical\(\|ui\|\) \{
                            let item_size = vec2\(self\.thumbnail_size, self\.thumbnail_size\);
                            
                            let response = ui\.allocate_ui\(item_size, \|ui\| \{
                                // Draw background
                                ui\.painter\(\)\.rect_filled\(ui\.max_rect\(\), 2\.0, Color32::from_gray\(30\)\);

                                if entry\.file_type == FileType::Directory \{
                                    ui\.painter\(\)\.text\(
                                        ui\.max_rect\(\)\.center\(\),
                                        Align2::CENTER_CENTER,
                                        "📁",
                                        FontId::proportional\(self\.thumbnail_size \* 0\.5\),
                                        Color32::WHITE,
                                    \);
                                \} else if crate::utils::is_image\(&entry\.path\) \{
                                    let uri = format!\("file://{}\", entry\.path\.to_string_lossy\(\)\);
                                    let img = egui::Image::from_uri\(uri\)
                                        \.max_size\(vec2\(self\.thumbnail_size, self\.thumbnail_size\)\);
                                    ui\.add\(img\.shrink_to_fit\(\)\);
                                \}
                            \}\);

                            if entry\.file_type == FileType::Directory \{
                                if response\.response\.clicked\(\) \{
                                    action = Some\(ExplorerAction::EnterDirectory\(entry\.path\.clone\(\)\)\);
                                \}
                            \} else if crate::utils::is_image\(&entry\.path\) \{
                                if response\.response\.clicked\(\) \{
                                    action = Some\(ExplorerAction::OpenFile\(entry\.path\.clone\(\)\)\);
                                }
                            }

                            // Text label below the thumbnail
                            ui\.label\(RichText::new\(&entry\.name\)\.size\(12\.0\)\);
                        \}\);"""

# The new content
new_content = r"""                        ui.vertical(|ui| {
                            let item_size = vec2(self.thumbnail_size, self.thumbnail_size);
                            
                            // 1. Allocate the space for the thumbnail
                            let (rect, response) = ui.allocate_at_least(item_size, egui::Sense::click());

                            // 2. Draw the background/container
                            ui.painter().rect_filled(rect, 2.0, Color32::from_gray(30));

                            // 3. Draw the content (icon or image)
                            if entry.file_type == FileType::Directory {
                                ui.painter().text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    "📁",
                                    FontId::proportional(self.thumbnail_size * 0.5),
                                    Color32::WHITE,
                                );
                            } else if crate::utils::is_image(&entry.path) {
                                // Use a child UI to display the image within the reserved rect
                                let mut child_ui = ui.child_ui(rect, ui.layer_id());
                                let uri = format!("file://{}", entry.path.to_string_lossy());
                                let img = egui::Image::from_uri(uri)
                                    .max_size(vec2(self.thumbnail_size, self.thumbnail_size));
                                child_ui.add(img.shrink_to_fit());
                            }

                            // 4. Handle interaction
                            if response.clicked() {
                                if entry.file_type == FileType::Directory {
                                    action = Some(ExplorerAction::EnterDirectory(entry.path.clone()));
                                } else {
                                    action = Some(ExplorerAction::OpenFile(entry.path.clone()));
                                }
                            }

                            // 5. Add the text label below the thumbnail
                            ui.label(RichText::new(&entry.name).size(12.0));
                        });"""

# Since regex with multi-line might be tricky, let's use a simpler approach.
# We'll find the line where the old block starts and ends.

lines = content.splitlines()
start_idx = -1
end_idx = -1

for i, line in enumerate(lines):
    if "let response = ui.allocate_ui(item_size, |ui|" in line:
        start_idx = i - 2 # To include the ui.vertical line
    if "ui.label(RichText::new(&entry.name).size(12.0));" in line:
        end_idx = i + 2 # To include the }); line

if start_idx != -1 and end_idx != -1:
    print(f"Found start: {start_idx}, end: {end_idx}")
    new_lines = lines[:start_idx] + new_content.splitlines() + lines[end_idx:]
    with open(file_path, 'w') as f:
        f.write("\n".join(new_lines) + "\n")
    print("Success")
else:
    print(f"Failed to find block. Start: {start_idx}, End: {end_idx}")
    # Debugging info
    for i in range(max(0, start_idx-5), min(len(lines), start_idx+5)):
        print(f"{i}: {lines[i]}")
    for i in range(max(0, end_idx-5), min(len(lines), end_idx+5)):
        print(f"{i}: {lines[i]}")

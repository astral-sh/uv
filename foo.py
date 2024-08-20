import re

# File paths
preview_changelog_file = 'PREVIEW-CHANGELOG.md'
changelog_file = 'CHANGELOG.md'

# Function to extract preview features from preview changelog content
def extract_preview_features(content):
    # Regex to find "### Preview features" and everything under it until the next "###" or end of content
    match = re.search(r'### Preview features\n(.*?)(\n### |\Z)', content, re.DOTALL)
    if match:
        return match.group(0).strip()
    return None

# Read the preview changelog file
with open(preview_changelog_file, 'r') as file:
    preview_changelog = file.read()

# Read the main changelog file
with open(changelog_file, 'r') as file:
    changelog = file.read()

# Split preview changelog into sections based on version
preview_sections = re.split(r'(## \d+\.\d+\.\d+)', preview_changelog)

# Rebuild the preview changelog as a dictionary
preview_dict = {}
for i in range(1, len(preview_sections), 2):
    version = preview_sections[i].strip()
    content = preview_sections[i + 1].strip()
    preview_features = extract_preview_features(content)
    if preview_features:
        preview_dict[version] = preview_features

# Go through the changelog and insert preview features at the end of the corresponding version section
updated_changelog = []
changelog_sections = re.split(r'(## \d+\.\d+\.\d+)', changelog)

for i in range(1, len(changelog_sections), 2):
    version = changelog_sections[i].strip()
    content = changelog_sections[i + 1].strip()
    
    if version in preview_dict:
        # Append preview features at the end of the current version content
        content += '\n\n' + preview_dict[version]
        print(f"Appended preview features to version {version} in CHANGELOG.md")

    updated_changelog.append(version)
    updated_changelog.append(content)

# Join the updated changelog sections back together
final_changelog = changelog_sections[0] + ''.join(f'{v}\n{c}\n\n' for v, c in zip(updated_changelog[0::2], updated_changelog[1::2]))

# Write the updated changelog back to the file
with open(changelog_file, 'w') as file:
    file.write(final_changelog.strip())

print("CHANGELOG.md has been successfully updated with preview features.")

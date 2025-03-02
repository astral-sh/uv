# /// script
# requires-python = ">=3.13"
# dependencies = [
#     "beautifulsoup4",
#     "requests",
# ]
# ///

import os
import sys
import requests
import shutil
from urllib.parse import urlparse
from pathlib import Path
from bs4 import BeautifulSoup

PYPI_SIMPLE_API = "https://pypi.org/simple"

def fetch_package_versions(package_name):
    """Fetch available versions of a package from PyPI Simple API."""
    url = f"{PYPI_SIMPLE_API}/{package_name}/"
    response = requests.get(url)
    
    if response.status_code != 200:
        print(f"Error: Failed to fetch package info for {package_name}")
        sys.exit(1)

    soup = BeautifulSoup(response.text, "html.parser")
    links = soup.find_all("a")
    links.reverse()
    
    return [(link.get("href"), link.text) for link in links if '2.2.3' in link.get('href')]

def download_files(package_name, download_links, dest_dir):
    """Download all distribution files for a package."""
    os.makedirs(dest_dir, exist_ok=True)

    for link, filename in download_links:
        file_url = link if link.startswith("http") else f"https://files.pythonhosted.org{link}"
        file_path = os.path.join(dest_dir, filename)

        if os.path.exists(file_path):
            print(f"Skipping {filename}, already exists.")
            continue

        print(f"Downloading {filename}...")
        response = requests.get(file_url, stream=True)
        
        if response.status_code == 200:
            with open(file_path, "wb") as f:
                for chunk in response.iter_content(8192):
                    f.write(chunk)
        else:
            print(f"Failed to download {filename}")

def generate_html_index(directory, package_name):
    """Generate index.html files for the Simple API."""
    # Package index
    package_dir = Path(directory) / package_name
    files = sorted(f.name for f in package_dir.glob("*") if f.is_file())
    
    package_index = "\n".join(f'<a href="{filename}">{filename}</a><br>' for filename in files)
    (package_dir / "index.html").write_text(f"<html><body>\n{package_index}\n</body></html>")

    # Root index
    root_index = f'<html><body><a href="{package_name}/">{package_name}</a><br></body></html>'
    (Path(directory) / "index.html").write_text(root_index)

def create_local_registry(package_name, output_dir="simple"):
    """Create a local Simple API-compatible registry."""
    output_path = Path(output_dir)
    package_dir = output_path / package_name

    print(f"Fetching package versions for {package_name}...")
    download_links = fetch_package_versions(package_name)
    
    print(f"Downloading distributions for {package_name}...")
    download_files(package_name, download_links, package_dir)

    print("Generating index.html files...")
    generate_html_index(output_path, package_name)

    print(f"Local registry created at {output_path.resolve()}")
    print(f"Run `cd {output_path} && python -m http.server` to serve it.")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python create_registry.py <package-name>")
        sys.exit(1)

    package_name = sys.argv[1]
    create_local_registry(package_name)


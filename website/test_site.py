from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit
import tomllib


class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.urls = []

    def handle_starttag(self, tag, attrs):
        attributes = dict(attrs)
        url = attributes.get('src' if tag in ('img', 'script') else 'href')
        if url and tag in ('a', 'link', 'img', 'script'):
            self.urls.append(url)


root = Path(__file__).parent / 'dist'
for page in root.rglob('*.html'):
    links = Links()
    links.feed(page.read_text())
    for url in links.urls:
        parsed = urlsplit(url)
        if parsed.scheme or parsed.netloc or not parsed.path:
            continue
        assert not parsed.path.startswith('/'), (page, url)
        target = page.parent / parsed.path
        assert target.is_file() or (target.is_dir() and (target / 'index.html').is_file()), (page, url)
required = {
    'commands': ['ghostty-wall create [PROFILE]', 'ghostty-wall edit [PROFILE]',
                 'ghostty-wall delete [PROFILE]', 'ghostty-wall history', 'ghostty-wall update --check'],
    'profiles': ['Save and use', 'Not now', 'Welcome', 'original image', 'version 2'],
    'terminal-browser': ['40×12', 'Create', 'Edit', 'Delete', 'Use', 'not live Ghostty reload'],
    'colors': ['Automatic', 'Customized', 'colors.palette.0'],
    'installation': ['install-release.sh', 'update --check'],
    'troubleshooting': ['Publication uncertainty', 'read-only', 'live'],
}
for name, snippets in required.items():
    text = (root / name / 'index.html').read_text()
    for snippet in snippets:
        assert snippet in text, (name, snippet)

package = tomllib.loads((root.parent.parent / 'Cargo.toml').read_text())['package']
for page in root.rglob('*.html'):
    assert f"Ghostty Wall {package['version']} · MIT License" in page.read_text(), page
assert f"Rust {package['rust-version']}" in (root / 'installation/index.html').read_text()
print('Site links, feature coverage and documentation version OK')

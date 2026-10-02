from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit
import re
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
for page in [root.parent.parent / 'README.md', root / 'installation/index.html']:
    text = re.sub(r'<[^>]+>', '', page.read_text())
    for snippet in ['curl -fsSL https://raw.githubusercontent.com/GiovanniCaiazzo01/Ghostty-wall/main/scripts/install-release.sh | bash',
                    'supported installation route', 'Linux x86_64',
                    '~/.local/bin', 'Inspect the installer']:
        assert snippet in text, (page, snippet)
    for removed in ['cargo install', 'Install with Cargo', 'Manual installation',
                    'Download the Linux binary', 'Try a source checkout']:
        assert removed not in text, (page, removed)
repository = root.parent.parent
readme = (repository / 'README.md').read_text()
assert len(readme.splitlines()) <= 80, 'Keep the README a short introduction, not a reference manual'
assert readme.index('media/screenshots/welcome.png') < readme.index('## Install')
assert 'internal previews, not live Ghostty reload' in readme
assert 'not live wallpaper switching' in readme
overview = (root / 'index.html').read_text()
assert overview.index('assets/ghostty-welcome.png') < overview.index('id="start-here"')
assert 'not live wallpaper switching' in overview
assert 'terminal-demo' not in overview, 'Use real captures, not a simulated terminal'
for source, deployed in [('welcome.png', 'ghostty-welcome.png'),
                         ('profile-browser.png', 'profile-browser.png'),
                         ('profile-browser.gif', 'profile-browser.gif')]:
    assert (repository / 'media/screenshots' / source).read_bytes() == (root / 'assets' / deployed).read_bytes(), source
for snippet in ['command -v ghostty-wall', '~/.cargo/bin']:
    assert snippet in (root / 'installation/index.html').read_text(), snippet
site_url = 'https://giovannicaiazzo01.github.io/Ghostty-wall/'
markdown = [*repository.glob('*.md'), *repository.joinpath('docs').rglob('*.md'), root.parent / 'README.md']
for document in markdown:
    for url in re.findall(r'\]\(([^)]+)\)', document.read_text()):
        parsed = urlsplit(url)
        if url.startswith(site_url):
            target = root / parsed.path.removeprefix('/Ghostty-wall/')
            if target.is_dir():
                target = target / 'index.html'
            assert target.is_file(), (document, url)
            if parsed.fragment:
                assert f'id="{parsed.fragment}"' in target.read_text(), (document, url)
        elif not parsed.scheme and not parsed.netloc and parsed.path:
            assert (document.parent / parsed.path).exists(), (document, url)
print('Repository and site links, curl-only installation guidance, feature coverage and documentation version OK')

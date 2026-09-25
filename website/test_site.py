from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit


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
print('Site links OK')

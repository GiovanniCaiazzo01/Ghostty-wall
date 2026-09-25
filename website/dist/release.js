async function updateLatestRelease() {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 8000);

  try {
    const response = await fetch(
      'https://api.github.com/repos/GiovanniCaiazzo01/Ghostty-wall/releases/latest',
      {
        headers: { Accept: 'application/vnd.github+json' },
        credentials: 'omit',
        signal: controller.signal,
      }
    );
    if (!response.ok) return;

    const release = await response.json();
    if (!release || release.draft || release.prerelease || typeof release.tag_name !== 'string') return;
    const version = release.tag_name.trim();
    if (!version) return;

    document.querySelectorAll('[data-release-version]').forEach(label => {
      label.textContent = version;
      label.closest('a').title = `Latest stable release: ${version}`;
    });
    document.querySelectorAll('[data-release-download]').forEach(label => {
      label.textContent = `Download ${version}`;
    });
  } catch {
    // The static latest-release links remain usable when GitHub is unavailable.
  } finally {
    clearTimeout(timeout);
  }
}

void updateLatestRelease();

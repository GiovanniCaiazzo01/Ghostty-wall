# Release Notes Template

Use this as the starting point for each GitHub Release.

## Summary

- 
- 
- 

## Verification

- `bash scripts/test.sh`
- `bash scripts/integration-test.sh`
- `shellcheck -x bin/ghostty-wall scripts/install.sh scripts/mac/install-mac.sh scripts/linux/install-linux.sh scripts/uninstall.sh scripts/test.sh scripts/integration-test.sh`
- Ubuntu CI green for the tagged commit
- Manual macOS smoke test completed

## Upgrade Notes

- 

## Known Limitations

- Releases currently ship source archives only.
- macOS validation is manual at release time.

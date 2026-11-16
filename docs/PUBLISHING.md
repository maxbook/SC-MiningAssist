# Publishing MiningMod

The public project is [maxbook/miningmod](https://github.com/maxbook/miningmod). Follow the release steps below for subsequent previews. The initial repository setup is retained for forks and maintainers creating a separate repository.

## Before the first push

1. Review README.md, LICENSE, the community data attribution and the exact Git file list. Keep .tools/, target/, dist/, artifacts/ and private screenshots out of Git.
2. Run the checks in CONTRIBUTING.md on Windows, including the local panel/HUD/lifecycle/OCR tracking checks.
3. Run scripts/package.ps1. It produces a portable ZIP and SHA256SUMS.txt in dist/, with an explicit file allowlist and an integrity check for every archive entry.
4. Create an **empty public** GitHub repository named miningmod on the intended account. Do not initialize another README or license on GitHub.

For the intended maxbook account, once the empty repository exists:

```powershell
git remote add origin https://github.com/maxbook/miningmod.git
git push -u origin main
```

If an origin already exists, inspect `git remote -v` before changing it. Use GitHub Desktop or your usual Git authentication. No credentials belong in the source tree.

Suggested About description: **Lightweight Rust OCR overlay for Star Citizen mining and salvage on Windows.**

Suggested topics: `star-citizen`, `mining`, `salvage`, `ocr`, `rust`, `windows`, `overlay`.

## Create the preview release

Wait for Windows CI to pass on main. Update the version in Cargo.toml/Cargo.lock, CHANGELOG.md, README.md and docs/RELEASE_NOTES.md together. Date the changelog entry and commit before tagging. The commands below use the first preview's version as an example; choose a new version for subsequent releases.

```powershell
git tag -a v0.1.0 -m "MiningMod 0.1.0 preview"
git push origin main
git push origin v0.1.0
```

The tag workflow checks that the tag matches Cargo.toml, reruns source checks, builds from the tagged source and creates a **draft prerelease** with the portable archive and checksum. Review its text and download/run the archive on another Windows installation if possible, then publish the draft from GitHub Releases. GitHub provides the source ZIP automatically.

The workflow deliberately does not publish or announce the release automatically. Rerunning after a draft already exists requires inspecting that draft; it does not silently overwrite existing release assets. Future stable releases can remove the `--prerelease` flag when appropriate.

## Verify a downloaded archive

```powershell
Get-FileHash .\MiningMod-v0.1.0-win64.zip -Algorithm SHA256
```

Compare the result with SHA256SUMS.txt from the same release. The checksum verifies bytes; it is not a code-signing certificate.

## Automation maintenance

CI uses read-only repository permissions. Only the tag release job requests contents:write; its token is exposed only to the release step, and checkout does not retain Git credentials. Actions are pinned to full commit IDs. Dependabot proposes dependency and action updates monthly; dependency PRs must also regenerate notices.

References: [checkout release](https://github.com/actions/checkout/releases/tag/v7.0.1), [upload-artifact release](https://github.com/actions/upload-artifact/releases/tag/v7.0.2), [GitHub CLI draft release options](https://cli.github.com/manual/gh_release_create), [Rust 1.100](https://blog.rust-lang.org/2026/11/12/Rust-1.100.0/).

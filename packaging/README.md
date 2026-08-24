# Packaging

Manifests for the third-party package managers magpie ships through. All point
at the GitHub release assets. On every new release, bump the version and the
`sha256` in each file (compute with `sha256sum` / `Get-FileHash`).

## Scoop (Windows, developer-facing)

`scoop/magpie.json` is mirrored into the **newdee/scoop-magpie** bucket repo.
Users install with:

```sh
scoop bucket add magpie https://github.com/newdee/scoop-magpie
scoop install magpie
```

On a new release, copy `scoop/magpie.json` into that bucket repo and push (or
let its `autoupdate` block pick the new version up).

## winget (Windows, official)

`winget/*.yaml` are the three manifest files (version / installer / locale).
Submit to **microsoft/winget-pkgs** with the official tool (uses your GitHub
token, opens the PR for you):

```sh
winget install wingetcreate
wingetcreate submit --token <gh-token> packaging/winget
```

or update in one step from the release URL:

```sh
wingetcreate update newdee.magpie --version 0.1.13 \
  --urls https://github.com/newdee/magpie/releases/download/v0.1.13/magpie_0.1.13_x64-setup.exe \
  --submit --token <gh-token>
```

Microsoft's CI validates the manifest; once merged, `winget install newdee.magpie` works.

## AUR (Arch Linux)

`aur/PKGBUILD` + `aur/.SRCINFO` define **magpie-bin** (unpacks the release
.deb). Push to the AUR with your AUR account's SSH key:

```sh
git clone ssh://aur@aur.archlinux.org/magpie-bin.git
cp packaging/aur/PKGBUILD packaging/aur/.SRCINFO magpie-bin/
cd magpie-bin
# regenerate .SRCINFO on a machine with makepkg: makepkg --printsrcinfo > .SRCINFO
git commit -am "magpie 0.1.13" && git push
```

Users then `yay -S magpie-bin` (or any AUR helper).

**Install**

| Computer | How |
|---|---|
| Linux Mint, Ubuntu, Debian | Download `crafthub_amd64.deb` (ARM: `crafthub_arm64.deb`) below and open it. Craft Hub then shows up in **Software Manager** and gets updates through **Update Manager**. |
| Other Linux, or no admin password | `curl -fsSL https://raw.githubusercontent.com/TymonDomanko/CraftHub/main/install.sh \| bash` |
| Windows | `irm https://raw.githubusercontent.com/TymonDomanko/CraftHub/main/install.ps1 \| iex` in PowerShell, or download `crafthub-windows-x86_64.exe` (ARM: `crafthub-windows-aarch64.exe`) |
| Mac | `curl -fsSL https://raw.githubusercontent.com/TymonDomanko/CraftHub/main/install.sh \| bash`, or download `crafthub-macos-universal.zip` and move `Craft Hub.app` to Applications |

If you already have Craft Hub, the **Update Craft Hub** button in the bottom-right corner installs
this version for you (with the `.deb`, Update Manager does).

**What's in Craft Hub**

- One-click Install / Update / Uninstall / Open for PhotoCraft, VectorCraft, DesignCraft,
  LightCraft, FilmCraft, EffectCraft, PDFCraft and ArtCraft
- Downloads are verified against each release's `SHA256SUMS.txt`
- Apps with updates are sorted to the top, with an Update all button
- Clean uninstalls, optionally including each app's settings and saved data
- Follows your light/dark appearance setting

See the commits since the previous release for what changed in this version.

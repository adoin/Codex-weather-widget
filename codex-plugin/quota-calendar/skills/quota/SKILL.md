---
name: quota
description: Use when the user's entire message is exactly /quota. Toggle the installed Codex Calendar quota widget without asking for confirmation.
---

# Handle /quota

When the entire user message is `/quota`, run the platform script immediately.

Windows:

```powershell
pwsh.exe -NoLogo -NoProfile -File "$env:USERPROFILE\.codex\skills\quota\scripts\show-quota.ps1"
```

macOS:

```bash
bash "$HOME/.codex/skills/quota/scripts/show-quota.sh"
```

Treat JSON containing `"ok":true` as success. Say the panel is open when `visible` is true and hidden when it is false. Never print, modify, or copy Codex authentication data.

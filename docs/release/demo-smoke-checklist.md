# BitCat Steam Demo Smoke Checklist

Use this checklist on a clean Windows machine or a clean Windows user profile before promoting a Demo build. The goal is to prove that BitCat can start, explain its AI and high-permission features, and keep user choices after restart.

## Test Setup

- Build source: `make dist` or the Steam depot candidate.
- Test data: use a clean profile with neither `%APPDATA%\bitcat` nor `%USERPROFILE%\.bitcat`, or move existing data aside before testing. Account for custom data directories if reusing a profile.
- Network/API: run one pass without an AI API key, and one pass with a valid key if available.
- Controller: connect the 8BitDo Micro when checking input paths.
- A successful CI or `dry_run` installer build proves compilation and packaging. Complete the interactive checks below on Windows before treating the Demo smoke pass as complete.

## First Launch Gate

- [ ] App starts without crashing on a clean profile.
- [ ] Settings opens automatically when `permissions.onboarding_completed` is false.
- [ ] The `它能做什么` (`home`) section is selected and the three-step trust wizard is visible.
- [ ] The wizard explains who the companion is, which capabilities the user can enable, and how to revoke them.
- [ ] Screenshot, camera, and computer-operation choices are off on a clean profile.
- [ ] Clicking `完成，去见它` marks onboarding complete and saves only the selected capabilities; it does not automatically enable Steam Demo mode.
- [ ] `稍后再说` leaves onboarding incomplete, so the wizard appears again on next launch.
- [ ] `再看一遍介绍` reopens the wizard after completion.
- [ ] Saving settings writes `%APPDATA%\bitcat\app_settings.json`.
- [ ] Restarting the app does not reopen onboarding after completion.

## Permission Defaults

- [ ] Screenshot observation is off by default and visible as a user-facing switch.
- [ ] Camera observation is off by default and does not request browser camera permission before the user enables it.
- [ ] Shell execution is off by default.
- [ ] File reading is off by default.
- [ ] Clipboard reading is off by default.
- [ ] Foreground/window control is off by default.
- [ ] Program launch is off by default.
- [ ] Hotkey sending is off by default.
- [ ] Agent Watch remote access is off by default.
- [ ] Turning a permission off persists after app restart.
- [ ] Confirming `全部收权` and then saving closes observation, computer-operation, and remote-watch permissions while keeping chat and local companion interactions available; changes persist after restart.

## Core Demo Paths

- [ ] Pet window appears with the default `cat-tabby` asset.
- [ ] Every currently offered pet preset can be selected and loaded; all 15 original packs and 11 `cat-pixel-*` trial packs are bundled, with `cat-tabby` remaining the default.
- [ ] AI chat with no API key shows a friendly fallback instead of an empty failure.
- [ ] AI chat with a valid key streams text into the bubble.
- [ ] Manual screenshot action works when screenshot observation is allowed.
- [ ] Screenshot observation stops after the permission switch is turned off and saved.
- [ ] Camera observation only starts after both camera settings and camera permission are enabled.
- [ ] Invasion starts from the panel.
- [ ] Invasion can be ended and started again without stale windows.
- [ ] Keyboard and controller input both work in the active game window.

## Packaging Checks

- [ ] Portable package contains `bitcat.exe`.
- [ ] Portable package contains `config/actions.yml`, `config/buttons.yml`, `config/panel_action.yml`, `config/prompts.yml`, and `config/user.yml`.
- [ ] Portable package does not include old non-cat pet asset packs.
- [ ] App can close from tray/menu and restart without orphaned windows.
- [ ] Logs are written under the expected BitCat data/log directory.
- [ ] Memory, screenshot, camera, and reminder data directories are discoverable from settings or documentation.
- [ ] The manual Release workflow succeeds with `dry_run=true`, produces MSI, NSIS, and portable ZIP artifacts, and skips `Create GitHub Release`.

## Remaining Release Gates

- [ ] Unified view/export/delete for memory, screenshots, camera records, logs, points, and reminders is complete. Per-entry memory/reminder deletion and diagnostic ZIP export do not complete this D3 requirement.
- [ ] Notification audio defaults satisfy the product spec. Current notification and reminder/Agent Watch sound switches default to enabled; release default-silence work remains pending.
- [ ] Revoking any permission takes at most two clicks. Current `全部收权` requires confirmation plus a separate save action, so this interaction still needs work.

## Failure Notes

Record every failed checkbox with:

- Build identifier or commit.
- Windows version and machine/profile type.
- Whether an API key was configured.
- Relevant log path and the last error line.
- Screenshot or short reproduction steps.

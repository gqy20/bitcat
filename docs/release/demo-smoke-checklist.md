# BitCat Steam Demo Smoke Checklist

Use this checklist on a clean Windows machine or a clean Windows user profile before promoting a Demo build. The goal is to prove that BitCat can start, explain its AI and high-permission features, and keep user choices after restart.

## Test Setup

- Build source: `make dist` or the Steam depot candidate.
- Test data: start with no existing `%APPDATA%\bitcat` directory, or move the old directory aside before testing.
- Network/API: run one pass without an AI API key, and one pass with a valid key if available.
- Controller: connect the 8BitDo Micro when checking input paths.

## First Launch Gate

- [ ] App starts without crashing on a clean profile.
- [ ] Settings opens automatically when `permissions.onboarding_completed` is false.
- [ ] The `发布与权限` tab is selected.
- [ ] The onboarding callout is visible.
- [ ] The gate summary shows screenshot observation, camera observation, high-risk tools, and remote watch status.
- [ ] Clicking `完成首次说明` marks onboarding complete and enables Steam Demo mode.
- [ ] Saving settings writes `%APPDATA%\bitcat\app_settings.json`.
- [ ] Restarting the app does not reopen onboarding after completion.

## Permission Defaults

- [ ] Screenshot observation is visible as a user-facing switch.
- [ ] Camera observation is off by default and does not request browser camera permission before the user enables it.
- [ ] Shell execution is off by default.
- [ ] File reading is off by default.
- [ ] Clipboard reading is off by default.
- [ ] Foreground/window control is off by default.
- [ ] Program launch is off by default.
- [ ] Hotkey sending is off by default.
- [ ] Agent Watch remote access is off by default.
- [ ] Turning a permission off persists after app restart.

## Core Demo Paths

- [ ] Pet window appears with the default `cat-tabby` asset.
- [ ] All 15 built-in `cat-*` assets can be selected from settings.
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

## Failure Notes

Record every failed checkbox with:

- Build identifier or commit.
- Windows version and machine/profile type.
- Whether an API key was configured.
- Relevant log path and the last error line.
- Screenshot or short reproduction steps.

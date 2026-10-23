# Combat Arms: options and settings

Source basis: the client's options screen (its four tabs and the key-capture flow), the client's player-profile manager (loading, defaults, saving, applying settings to the engine), the performance-preset table, the client's mouse-sensitivity and camera-rotation code, the radar zoom and HUD toggle handlers, the slider widget of the UI library, the engine's input reader, sound manager and gamma ramp; checked against the plaintext development seed profile that ships with the reference material and against the profile templates and key-name list named by the client. The Steam build's profile templates are encrypted and were not read (Open questions 1 and 2). The options layouts were not read; their file names and control names come from the code that binds them.
Status: draft

Sections: Summary; Units and conventions; Constants; Behavior (P: the options screen and its buttons; K: controls and key binding; M: mouse; W: slider widget; G: graphics; S: sound and voice; O: other (social) tab; H: options with no page; F: storage, loading and saving; L: layout files); Per-frame order; Edge cases; Quirks; Answers to other specs' open questions; Test cases; Open questions.

---

## Summary

- There is one options screen, opened from the lobby's start menu as a modal screen over the menus. It has four tabs: **Graphic**, **Sound**, **Control** and **Other**, plus OK, Cancel (and a close button that acts as Cancel) and Default. It cannot be opened during a match; in a match the only setting keys are the bindable mouse-sensitivity up/down keys, the radar zoom keys and the HUD on/off key.
- Every setting lives in one plain key/value profile file in the game folder, `Profiles\player.txt` (role: *the player profile*). A missing profile is created from a template (`Profiles\High_Default.prf`, role: *the default profile*). Social options on the Other tab are stored on the account service, not in the profile.
- Sliders run 0 to 100. Mouse sensitivity S turns into an engine axis scale of 0.00001 + 0.000225·S radians per mouse count; with the stock input path a mouse count turns the view by 1.5 times that, divided by 1 + 3L while zoomed to level L.
- Sound volume G (0 to 100) is applied twice: once as the sound driver's master level G/100 and once inside every sound's own level, so loudness falls roughly with (G/100)². Music is a separate stream at musicvolume/100.
- There is no gameplay page: no crosshair style, colour or size setting, no kill-feed or name-tag setting. The crosshair comes from weapon data and items (`hud_crosshair.md`). Blood is a Graphic-tab switch.

## Units and conventions

- **Angles:** radians unless written in degrees. View yaw and pitch are accumulated in radians.
- **Mouse counts:** raw relative mouse motion as reported by the system's mouse device, per frame.
- **Time:** wall-clock seconds or milliseconds. The options screen and the mouse look run once per rendered frame; nothing here runs on the 10 Hz simulation tick of `movement.md`.
- **Key codes:** the profile stores keys as scan-code tokens `##n` (n decimal, the standard PC set-1 scan code with 200+ codes for the extended keys, e.g. `##17` = W, `##201` = Page Up). Mouse buttons are `##3` (left) and `##4` (right) on the mouse device. The wheel is stored as `#U` (wheel up) or `#D` (wheel down).
- **Profile file:** sections in square brackets, one `Key = value` per line, `//` comments. Strings are quoted, floats may carry a trailing `f`, booleans are `TRUE`/`FALSE` (integers 0/1 are used for many booleans as well). Lookups of section and key names ignore case.
- **Binding value:** one string per action, `"kbd@mouse@joystick"`: three device slots separated by `@`, each either a token or a single space for "nothing" (e.g. `"##17@ @ "` = W on the keyboard only, `" @##3@ "` = left mouse button only).

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| sens_base | 0.00001 | rad/count | axis scale at sensitivity 0 |
| sens_step | 0.000225 | rad/count | added per sensitivity point |
| sens_default | 5 | points | when the profile has no `Sensitivity` |
| sens_key_min, sens_key_max | 1, 99 | points | limits of the in-match up/down keys (the slider allows 0 to 100) |
| no_smoothing_gain | 3 | | count multiplier on the stock (no-smoothing) mouse path |
| look_half | 0.5 | | final factor on mouse yaw and pitch |
| zoom_divisor | 1 + 3L | | L = zoom level 0, 1, 2 (`weapon_classes.md` 7.7) |
| input_rate | 12 | ms | `InputRate`, used only by the legacy smoothing path |
| slider_range | 0 to 100 | | every slider widget, set by code, not by layout |
| slider_arrow_step | max(1, ⌊W/100⌋) | px | thumb move per click on a slider arrow; W = track width |
| gamma_min, gamma_max | 0.5, 6.0 | | gamma at slider 0 and 50 |
| gamma_break | 16 | slider units | slider value that gives gamma 1.0 |
| gamma_default | 1.22 | | when the profile has no `ScreenGamma` |
| volume_range | 0 to 100 | | sound, music, microphone and speaker |
| default_class_mult | 0.75 | | sound class "default" multiplier |
| weapons_class_mult, speech_class_mult | 1.0, 1.0 | | the two other sound classes |
| radar_scale_min, radar_scale_max | 0.5, 2.0 | | radar zoom limits |
| radar_scale_step | 0.01 | per frame | while a radar zoom key is held |
| sens_message_time | 5000 | ms | chat line shown after an in-match sensitivity change |
| menu_mode | 1024 x 768 x 32 | px, bits | fallback resolution and the menus' fixed mode (`ui_system.md` S1) |
| min_listed_mode | 800 x 600 | px | smaller display modes are not listed |
| bandwidth targets | 5,120,000 client, 35,840 server | | written at every multiplayer apply, whatever the profile says |

## Behavior

### P. The options screen and its buttons

- **Opening.** The screen is a modal screen over the current menu screen, opened from the start-menu popup (lobby top bar "menu"). On opening:
  1. The current profile is saved to the profile file (so Cancel can go back to exactly this state).
  2. Every tab reads its current values from the engine's console variables and the profile (the tabs are visited from Other back to Graphic, which leaves Graphic as the visible tab).
- **Tabs.** Clicking a tab button shows that tab's window and hides the others. The tab buttons are `tab_graphic`, `tab_sound`, `tab_control`, `tab_other`; the windows are `GraphicWnd`, `SoundWnd`, `ControlWnd`, `OtherWnd`.
- **OK** (`button_ok`): writes every tab's values to the engine (Graphic, Sound, Control, Other in that order), applies the key bindings, copies the console values into the profile, saves the profile file, saves the music-list choices, closes.
- **Cancel / close** (`button_cancel`, `button_close`): re-reads the profile file (the copy saved on opening) and re-applies controls, game options, sound and display from it; restores the music-list checks; closes. Anything that was previewed (gamma, volumes, a window-mode display change) goes back.
- **Default** (`button_default`), acting on the **visible tab only**, without closing:
  - Graphic: display and performance values from the default profile (resolution, depth, anti-aliasing, gamma, cursor, vsync, quality preset and custom details), applied at once.
  - Sound: sound values from the default profile, applied at once; music-list checks back to the defaults.
  - Control: all bindings and the mouse values from the default profile, applied at once.
  - Other: the social switches back to the defaults kept in the profile's `[OtherOption]` section (O); nothing is read from the default profile.
  - The tab then re-reads its controls. The values only reach the profile file with OK; Cancel undoes Default too.
- **When values apply** (column "Applies" in the tables below): *at once* = the engine changes while the control is moved (a preview, undone by Cancel); *OK* = on OK; *next match* = when the next match starts; *restart* = next start of the game; *next map* = on the next entry into a world.

### K. Controls and key binding (Control tab)

#### K1. The bindable actions

The Control tab lists these 38 rows (layout control name of the row, the action's stored name in the profile's `[Bindings]` section, the stock key). The row label shown in game comes from the layout (Open question 3).

| # | Row control | Profile action | What it does | Stock key |
|---|---|---|---|---|
| 1 | `IncreaseSens` | `SensitivityInc` | sensitivity +1 (M3) | ] |
| 2 | `DecreaseSens` | `SensitivityDec` | sensitivity −1 (M3) | [ |
| 3 | `Forward` | `Forward` | move forward | W |
| 4 | `Back` | `Backward` | move back | S |
| 5 | `Left` | `Left` | strafe left | A |
| 6 | `Right` | `Right` | strafe right | D |
| 7 | `Action` | `Activate` | use / pick up | E |
| 8 | `Jump` | `Jump` | jump | Space |
| 9 | `Walk` | `Speed` | walk (`movement.md`) | C |
| 10 | `Crouch` | `DuckLock` | crouch (behaves as hold, `movement.md`) | Left Ctrl |
| 11 | `Sprint` | `Dash` | sprint | Left Shift |
| 12 | `Fire` | `Fire` | fire | left mouse button |
| 13 | `Reload` | `Reload` | reload | R |
| 14 | `FiringMode` | `TripleShotMode` | fire-mode toggle | F |
| 15 | `Zoomin` | `ZoomIn` | zoom / alternate attack | right mouse button |
| 16 | `Map` | `FullTacticalMap` | large map | M |
| 17 | `PreviousWeapon` | `Prev_Weapon` | previous weapon (`inventory.md` 4) | Q |
| 18 | `PrimaryWeapon` | `PrimaryWeapon` | class key 1 | 1 |
| 19 | `SecondaryWeapon` | `SecondaryWeapon` | class key 2 | 2 |
| 20 | `MeleeWeapon` | `MeleeWeapon` | class key 3 | 3 |
| 21 | `SupportItem` | `MiscWeapon` | class key 4 (throw) | 4 |
| 22 | `SpecialItem` | `SpecialWeapon` | class key 5 | 5 |
| 23 | `MissionItem` | `ObjectiveWeapon` | class key 6 | 6 |
| 24 | `Chatting` | `SendMessage` | open chat input | Enter |
| 25 | `SocialMsg` | `RadioRequest` | radio menu "request" | N |
| 26 | `TalkInGamePlay` | `VoiceChatToggle` | voice push-to-talk toggle | T |
| 27 | `TacticalMsg` | `RadioAnswer` | radio menu "answer" | V |
| 28 | `ReportMsg` | `RadioReport` | radio menu "report" | B |
| 29 | `HUDOnOff` | `HUDOnOff` | HUD on/off (H) | Page Up |
| 30 | `VideoCapture` | `VideoCapture` | start/stop recording | Page Down |
| 31 | `Help` | `HelpScreen` | help popup toggle | F1 |
| 32 | `radarmap_reduct` | `DecRadarScale` | radar zoom out (H) | − |
| 33 | `radarmap_enlarge` | `IncRadarScale` | radar zoom in (H) | = |
| 34 | `SuperSoldier` | `UseMutationItem` | use the super-soldier item | Left Alt |
| 35 | `NPC_Shop` | `CommandNPCShop` | co-op NPC shop | G |
| 36 | `NPC_Order` | `CommandNPCOrder` | co-op mercenary orders | H |
| 37 | `Use_Perk` | `CommandUsePerk` | use ability | ` (grave) |
| 38 | `QuestState` | `QuestState` | mission/quest status | J |

- The stock keys are those of the plaintext stock profile in the reference material; they agree with `inventory.md` 2 and `movement.md` (walk C, sprint Left Shift, crouch Left Ctrl). Whether the Steam build's default profile has the same keys is Open question 1.
- Further actions are part of the same bindable set but have no row: `NightVision` (unbound), `Duck` (hold-crouch, unbound), `TeamChat` (F3) and `BroadCastChat` (F2). They are loaded, applied and saved like the rows; they can only be changed by editing the profile.
- **Fixed keys** that are not in the list: after every apply of the bindings, keys 1 to 6 are additionally bound to `Choose_1` … `Choose_6` (menu-line picks for the radio and emote menus, `inventory.md` 2). Esc, Pause and F8 are reserved (Esc menu, F8 voice; `ui_system.md` I2) and can never be captured. The stock profile also holds entries the list does not use (`Choose_7`…`Choose_0`, `ClanChat`, `TeamState`, `KickUser`, `HelpToggle`, `BuyWeapon`, …); they are kept in the file untouched (Open question 4).

#### K2. Key text in the list

- Each row shows its keyboard key, then `", "` and its mouse button, then `", "` and the joystick control, skipping empty slots. A row with nothing shows `N/A`.
- Key and mouse-button names come from the key-name attribute file (`KeyMappingString.txt` in the attributes archive): sections `Keyboard0`, `Keyboard1`, … and `Mouse0`, `Mouse1`, …, read until the first missing number, each with `RealName` (the stored token, e.g. `##42`) and `TriggerName` (the text shown, trimmed). A token missing from the file shows the device's own control name instead.

#### K3. Rebinding a key

1. **Start.** Left-click a row (not while a capture is already waiting). The row's text becomes empty and the screen starts recording raw input from keyboard, mouse and joystick (up to 8 events per frame).
2. **Clear.** On the first frame on which any input event at all is recorded (including the release of the click that started capture, or a mouse move), the action's bindings on **all three devices** are erased.
3. **Pick.** The frame's events are scanned in order; key-up events are skipped. The first key-down event decides:
   - **Mouse wheel** (either direction): the token `#U` or `#D` is taken away from any other action, then given to this action's mouse slot. Capture ends.
   - **Mouse axis or joystick axis movement:** ignored; capture keeps waiting (the erase of step 2 has already happened).
   - **Key or button:** if the key is remappable (below), the token is taken away from every action that has it in the same device slot (**conflict rule: the new action steals the key**), then written into this action's slot for that device (keyboard and mouse as `##code`, joystick as the control's name). If it is not remappable, nothing is written. Either way capture ends.
4. After capture ends, the list text is refreshed. The change lives in the profile's in-memory binding table; it reaches the engine on OK (bindings are re-applied from the table) and is lost on Cancel.

- **Remappable keys:** every key except Esc, Pause and F8, and except a key the engine already has bound to an action that is neither in the list of K1 nor one of `Choose_1`…`Choose_6`.
- **One key per device per action.** An action holds at most one keyboard key, one mouse button and one joystick control. Binding a second keyboard key replaces the first.
- **Two actions on one key** can only come from the profile file (the UI always steals). Both actions are then bound and both fire.
- **Resetting:** Default on the Control tab (P). There is no per-row reset.

#### K4. Missing or empty bindings when the profile loads

For each action of K1 (plus the four without rows), in list order:

1. Read its value from `[Bindings]` and split it into the three device slots.
2. If at least one slot holds a token, use it as is.
3. Otherwise, if the action has a built-in fallback key, apply it:
   - **Always re-filled:** `Prev_Weapon` → Q, `SpecialWeapon` → 5, `TripleShotMode` → F, `TeamChat` → Delete (`##211`). The fallback is written even if another action already uses that key.
   - **Re-filled only if free** ("free" checks only the actions earlier in the list, which are already loaded): `UseMutationItem` → Left Alt, `VideoCapture` → Page Down, `HUDOnOff` → Page Up, `CommandNPCShop` → G, `CommandNPCOrder` → H, `CommandUsePerk` → `, `QuestState` → J. If another action already holds that key: when the entry was absent or an empty string (not merely three blank slots), or when defaults are being loaded, **the whole `[Bindings]` section is replaced by the default profile's** and loading restarts from there; otherwise the action stays unbound.
   - The sensitivity keys also have fallbacks in the data (] and [), but they are never applied (Quirk 5).
4. Every other action with no token stays unbound.

### M. Mouse

#### M1. Settings on the Control tab

| Shown control | Stored (section `[Controls]`) | Type | Range, step | Default when missing | Console variable | Applies |
|---|---|---|---|---|---|---|
| Mouse sensitivity slider `SliderBar_Mousesens`, value text `SliderBar_value` | `Sensitivity` | integer | 0 to 100, slider units (W) | 5 | `MouseSensitivity` | OK |
| Invert mouse `InvertMouse_On` / `InvertMouse_Off` | `InvertMouse` | 0/1 | | 0 | `MouseInvertY` | OK |
| Radar rotation `mapRotaion_On` / `mapRotaion_Off` | `RotateRadar` | 0/1 | | 0 | `RotateRadar` | next map |

Not on any page: `InputRate` (default 12, console `InputRate`, legacy smoothing only), `MouseLook` (always forced on at load), keyboard turning rates `NormalTurn` 15, `FastTurn` 23, `LookUp` 25 (stored as tenths: console value = stored/10), `AutoCenter` 0, joystick `UseJoystick` 0, `Axis0`…`Axis5`, `POV0`…`POV3`. There is no mouse acceleration option; the client's adaptive-mouse curve exists but its console switch is off by default (Open question 8).

#### M2. From mouse counts to view angles

Applying sensitivity sets the engine's scale on both mouse axes:

```math
k(S) = 0.00001 + 0.000225\,S \quad \text{(rad per count)}
```

Each frame the engine turns the frame's mouse counts into axis offsets. On the **stock path** (no smoothing):

```math
a_x = 3\,k(S)\,c_x, \qquad a_y = 3\,k(S)\,c_y
```

where c_x, c_y are the counts that arrived since the previous frame. The view then changes by

```math
\Delta\text{yaw} = \frac{0.5\,a_x}{1 + 3L}, \qquad \Delta\text{pitch} = \pm\frac{0.5\,a_y}{1 + 3L}
```

- So one count turns the unzoomed view by 1.5·k(S) rad: at S = 5, 0.0017025 rad (0.0975°), and a full turn takes about 3690.6 counts. Zoom divides this by 4 (level 1) or 7 (level 2), as in `weapon_classes.md` 7.7.
- **Invert:** pitch is added with the sign flipped when `MouseInvertY` is 1.
- **No look while a popup is up:** the offsets are zero on any frame on which the Esc menu, the user-report box, a message box, the user tooltip, the co-op mercenary popup or the co-op shop is open.
- A yaw or pitch change of a full turn or more in one frame is reduced modulo 2π.
- Pitch limits are those of `movement.md` (eye) and the camera code; this spec does not change them.
- **Legacy smoothing path** (console `MouseSmoothingEnable` 1): no ×3. Each axis keeps its last three samples s₀ (newest), s₁, s₂ and uses s₀ + (InputRate/100)·((s₀+s₁+s₂)/3 − s₀) = 0.88·s₀ + 0.04·(s₀+s₁+s₂) at InputRate 12; a zero sample is pushed when no event came for more than InputRate/10 ms. Which path the shipping client used is Open question 7.

#### M3. Sensitivity keys in a match

- `SensitivityInc`: if S + 1 < 100, S becomes S + 1; otherwise S stays. `SensitivityDec`: if S − 1 > 0, S becomes S − 1; otherwise S stays. So the keys move S within 1 to 99, even though the slider reaches 0 and 100.
- The new S is applied at once (M2) and a chat line "sensitivity changed to S" (string from the string table, colour from the HUD layout attributes' mouse-sensitivity colour) is shown for 5000 ms. The line is shown even when S did not change.
- The value is written to the profile file when it is next saved (options opened, OK, or game exit).

### W. Slider widget (all sliders)

Every slider of this screen (sensitivity, gamma, music, effects, microphone, speaker) is the same widget with a fixed range of 0 to 100:

- **Track width** W (px) = position of the right arrow − position of the left arrow − width of the left arrow − width of the thumb. The thumb's offset d runs from 0 to W.
- **Reading:** value = ⌊d · 100 / W⌋, recomputed whenever the thumb moves; a change notifies the screen at once.
- **Setting from a stored value v:** the thumb is placed at d = ⌊(v + 1) · W / 100⌋, then the value is read back as above. So the read-back is v + 1 when (v + 1)·W/100 is a whole number, and v otherwise (Quirk 2), capped at 100 by the track end.
- **Arrows:** each click moves the thumb by max(1, ⌊W/100⌋) px left or right. **Clicking the track** moves the thumb so its left edge is at the click. **Dragging** the thumb follows the mouse, clamped to the track.
- When W < 100, not every value can be reached by dragging; when W > 100, several pixels give the same value.

### G. Graphics (Graphic tab)

#### G1. Settings

| Shown control | Stored | Type, values | Default when missing | Console variable(s) | Applies |
|---|---|---|---|---|---|
| Full screen / Window `check_full`, `check_window` (window-mode builds only) | `[Display] windowed` | 0/1 | 0 | `windowed` | at once (window-mode builds) |
| Resolution drop-down `drop_resolution`, text `droptext_resolution` | `[Display] ScreenWidth`, `ScreenHeight` | a listed mode (G2) | 1024 x 768 | `ScreenWidthSetting`, `ScreenHeightSetting` | next match (full-screen builds); at once (window-mode builds) |
| Colour depth `check_16bit`, `check_32bit` | `[Display] ScreenBitDepth` | 16 or 32 | 32 | `ScreenDepthSetting` | as resolution |
| Brightness slider `sliderbar_gamma`, value text `sliderbar_value_gamma` | `[Display] ScreenGamma` | float, via G3 | 1.22 | `GammaR`, `GammaG`, `GammaB` (all equal) | at once |
| Anti-aliasing drop-down `drop_antialias`, text `droptext_antialiasing` | `[Display] AntiAlias` | sample count, shown as "NX" | 0 | `AntiAliasFSOverSample` (and `AcceptMSAA` = 1 when nonzero) | as resolution |
| Quality `check_low`, `check_mid`, `check_high`, `check_custom` | `[Settings] PerformanceSetting` | 0 Low, 1 Mid, 2 High, 3 Custom | 0 | `PerformanceSetting` | OK |
| Shell casings `check_emptycartridges_off/_on` | `[Settings] ShellCasings` | 0/1 | 0 | `ShellCasings` | OK (Custom only) |
| Tracers `check_viewflare_off/_on` | `[Settings] Tracers` | 0/1 | 0 | `Tracers` | OK (Custom only) |
| Shadows `check_shadow_off/_low/_high` | `[Settings] ShadowDetail` | shown 0, always disabled | 0 | `ShadowDetail` | (G4) |
| Anisotropic filtering `check_anisotropic_off/_on` | `[Settings] AnisotropicFiltering` | 0/1 | 0 | `anisotropic` | OK (Custom only) |
| Trilinear filtering `check_trilinear_off/_on` | `[Settings] TrilinearFiltering` | 0/1 | 0 | `trilinear` | OK (Custom only) |
| Environment `check_envmap_off/_low/_high` | `[Settings] EnvironmentalDetail` | 0, 1, 2 | 0 | `EnvironmentalDetail` | OK (Custom only) |
| Character detail `check_chardetail_low/_mid/_high` | not written | shown 2, always disabled | | | never |
| Shader `check_shader_off/_on` | `[Settings] Shader` | 0/1 | 0 | `Shader` | OK (Custom only) |
| Bloom `check_bloom_off/_on` | `[Settings] EnableBloom` | 0/1 | 0 | `EnableBloom` | OK (Custom only) |
| Effects level `check_fxlevel_low/_mid/_high` | `[Settings] FXDetail` | 0, 1, 2 | 0 | `FXDetail` | OK (Custom only) |
| Blood `check_blood_off/_on` | `[Game] Gore` | 0/1 | 0 | `Gore` | OK |

- Hidden display values: `[Display] HardwareCursor` (FALSE) and `VSync` (FALSE; console `VSyncOnFlip`; changing it restarts the renderer), `RadarScale` (H).
- The custom detail block (`detail`) is visible only while Custom is checked; leaving Custom scrolls the tab's list (`graphic_list`, scrollbar `graphic_scroll`) back to the top and disables the scrollbar.

#### G2. The resolution list

- Built once from the display modes the renderer reports. A mode is listed when it is at least 800 x 600, has the same pixel format as the first reported mode, has 16 or 32 bits per pixel, and (when the current renderer uses hardware transform and lighting) supports hardware transform and lighting. Duplicates (same width, height, depth, format) are dropped.
- Sorted by width descending, then height descending, then format, then depth descending. A width and height present at both 16 and 32 bits appears twice.
- Each entry is shown with the string-table resolution format (width and height). The current entry is the one matching `ScreenWidthSetting`, `ScreenHeightSetting`, `ScreenDepthSetting` (defaults 1024, 768, 32).
- If no 16-bit mode exists, the 16-bit button is disabled and a stored 16 becomes 32.
- The anti-aliasing list holds the multisample counts the renderer supports for the current mode, in ascending order. When the renderer cannot report them the list is empty and disabled.
- **When applied:** in full-screen builds the menus stay at 1024 x 768 (`ui_system.md` S1); the stored mode is set when a match starts and the menus' mode comes back afterwards. In window-mode builds the window style (captioned window or borderless popup) and mode change at once when any of window mode, resolution, depth or anti-aliasing changes.
- **At load**, a stored width, height and depth that is not among the reported modes is replaced by 1024 x 768 x 32.

#### G3. Brightness (gamma)

The slider value s (0 to 100) maps to gamma γ by

```math
\gamma(s) = \begin{cases} 0.5 + 0.5\sqrt{s/16} & s < 16 \\ 1 + 5\left(\dfrac{s-16}{34}\right)^2 & s \ge 16 \end{cases}
```

and a stored γ is shown at slider position

```math
s(\gamma) = \begin{cases} \left\lfloor 0.5 + 16\left(\dfrac{\gamma-0.5}{0.5}\right)^2 \right\rfloor & \gamma < 1 \\ \left\lfloor 0.5 + 16 + 34\sqrt{(\gamma-1)/5} \right\rfloor & \gamma \ge 1 \end{cases}
```

- The scale was designed for 50 steps: s = 50 gives γ = 6.0, but the widget goes to 100, and s = 100 gives γ ≈ 31.52 (Quirk 3).
- The display ramp for each channel, input level i = 0…255, is 65535·(i/255)^(1/γ), truncated. γ > 1 brightens.
- Moving the slider applies the ramp at once; Cancel restores the stored γ.

#### G4. Quality presets

Choosing Low, Mid or High writes these console values on OK (Custom writes the custom block's values instead; values not on the custom block keep their current setting):

| Stored `[Settings]` name | Console variable | Low | Mid | High |
|---|---|---|---|---|
| ShellCasings | ShellCasings | 0 | 1 | 1 |
| Tracers | Tracers | 0 | 1 | 1 |
| DynamicLight | dynamiclightworld | 1 | 1 | 1 |
| ShadowDetail | ShadowDetail | 0 | 1 | 2 |
| DetailTextures | detailtextures | 0 | 0 | 1 |
| AnisotropicFiltering | anisotropic | 0 | 1 | 1 |
| TrilinearFiltering | trilinear | 0 | 1 | 1 |
| EnvironmentalDetail | EnvironmentalDetail | 0 | 1 | 2 |
| TripleBuffering | triplebuffer | 0 | 1 | 1 |
| PolyGridFresnel | fresnelpolygrids | 0 | 1 | 1 |
| PolyGridBumpmap | bumpmappolygrids | 0 | 1 | 1 |
| FXDetail | FXDetail | 0 | 1 | 2 |
| ModelLODOffset | ModelLODOffset | 0 | 1 | 2 |
| Shader | Shader | 0 | 0 | 1 |
| EnableBloom | EnableBloom | 0 | 0 | 0 |
| EnvironmentMapping | envmapenable | 1 | 1 | 1 |
| EnvironmentBumpMapping | envbumpmapenable | 1 | 1 | 1 |
| DetailLevel | DetailLevel | 2 | 2 | 2 |
| MusicQuality | MusicQuality | 1 | 1 | 1 |
| SoundQuality | sound16bit | 1 | 1 | 1 |
| MusicActive | MusicActive | 1 | 1 | 1 |
| PreCacheAssets | PreCacheAssets | 1 | 1 | 1 |
| Polygrid | Polygrid | 1 | 1 | 1 |
| LightModels | LightModels | 1 | 1 | 1 |

Derived values written with every preset or custom apply:

- **Shadows:** ShadowDetail 0 turns model projected shadows off. ShadowDetail 1 or 2 turns them on with (blur, texture size, back-face cull, shadows per frame) = (0, 128, 0, 0) for 1 and (1, 256, 1, 2) for 2.
- **Effects:** the client effect level and the debris effect level both equal FXDetail (0, 1 or 2).
- **Environment:** EnvironmentalDetail 0 sets the snow density scale to 0.75, any other value to 1.0; scatter and snow stay on.
- **Texture detail:** DetailLevel 2 sets the texture group offsets (regular, large, extra-large) to 1, 1, 2; these are also stored as `[Details] RegularTex`, `LargeTex`, `XLargeTex`.
- **Machine limits:** triple buffering and music are forced off when the machine does not support them.
- **Shader-capable machines with a preset (not Custom):** after the preset is written, the client effect level and debris effect level are forced to 1 (Quirk 4).
- `PerformanceSetting` is written to the console and to `[Settings]`.
- When connected to a server, the client sends the server its environment detail and whether it pre-caches assets.

**Rules between the custom switches** (enforced as they are clicked, and on load):

- Anisotropic on turns trilinear off, and trilinear on turns anisotropic off.
- Shader on: bloom becomes selectable, effects level Low is disabled and a Low effects level becomes Mid, effects level High becomes selectable.
- Shader off: bloom is set off and disabled, effects level High is disabled and a High effects level becomes Mid, effects level Low becomes selectable.
- On a machine without shader support, the shader and bloom switches and effects level High are disabled.
- Shadows are always shown off and cannot be changed, so a Custom apply always writes ShadowDetail 0. Character detail is always shown High and is never written.

#### G5. Blood

- The switch writes the console `Gore` on OK and `[Game] Gore` on save. Gore off removes blood effects (`effects_sounds.md`).
- In a low-violence build, Gore is always 0.
- If the account's age (from the service) is under 18 in the US and Korean builds, the switch shows off and is disabled. The European, Brazilian and Russian builds have no age limit.

### S. Sound and voice (Sound tab)

#### S1. Settings

The tab has two sub-pages, **General** (`general_window`, button `button_general`) and **Voice** (`voice_window`, button `button_voice`). The Brazilian build hides both sub-page buttons.

| Shown control | Stored `[Sound]` | Type, range | Default when missing | Console variable | Applies |
|---|---|---|---|---|---|
| Music on/off `bgm_on`, `bgm_off` | `musicenable` | bool | TRUE | `enablemusic` | at once |
| Music volume `bgm_volume_sliderbar`, text `bgm_volume_value` | `musicvolume` | 0 to 100 (W) | 0 | `musicvolume` | at once |
| Music order `bgm_rotation`, `bgm_random` | `bgmrotation` | TRUE = in order, FALSE = random | TRUE | `bgmrotation` | at once |
| Music list `list_music` (one check `check_all` per track row `line_music`) | music user profile file | per track | | | OK (music and ambience spec) |
| Effects on/off `sound_on`, `sound_off` | `soundenable` | bool | TRUE | `enablesound` | at once |
| Effects volume `sound_volume_sliderbar`, text `sound_volume_value` | `soundvolume` | 0 to 100 | 0 | `soundvolume` | at once |
| Voice chat on/off `voicechat_on`, `voicechat_off` | `EnableVoiceChat` | bool | FALSE | `EnableVoiceChat` | at once (also sent to the service) |
| Game sound during voice `gamesoundforvoice_on/_off` | `SoundAdjustVolume` | 100 (on) or 0 (off) | 0 | `SoundAdjustVolume` | OK |
| Test microphone `btn_testmic` | | held: microphone open while the mouse button is down | | | at once |
| Microphone volume `mic_volume_sliderbar`, text `mic_volume_value` | `MicVolume` | 0 to 100 | 0 | `MicVolume` | at once |
| Microphone device `mic_device_combo`, text `droptext_microphone` | `MicDeviceIndex` | index into the voice system's device list | 0 | `MicDeviceIndex` | at once |
| Speaker volume `speaker_volume_sliderbar`, text `speaker_volume_value` | `SpeakerVolume` | 0 to 100 | 0 | `SpeakerVolume` | at once |
| Speaker device `speaker_device_combo`, text `droptext_speaker` | `SpeakerDeviceIndex` | index | 0 | `SpeakerDeviceIndex` | at once |

- Hidden `[Sound]` values: `weaponssoundmultiplier` 1.0, `speechsoundmultiplier` 1.0, `defaultsoundmultiplier` 0.75, `sound16bit` 0, `MusicQuality` 0.
- The voice controls other than the on/off switch are enabled only while voice chat is on. In builds without voice chat, every voice control is disabled. Device names longer than 24 characters are shown as their first 22 characters followed by "...". While the voice system is not ready the device texts read "finding"; an index past the list's end shows `N/A`.
- Opening the Sound tab enters the voice system's echo channel; leaving the screen leaves it.

#### S2. How the volumes scale gain

- **Effects master.** With effects on, the sound driver's master level is G/100 (G = `soundvolume`, clamped to 0…100 after conversion to an unsigned 16-bit integer, so a negative stored value becomes 100). With effects off, the master level is 0.
- **Each sound** has its own volume r (0 to 100, from its data; see `effects_sounds.md`) and a class: weapons (multiplier 1.0), speech (1.0, and the global volume is **not** applied inside the sound) or default (0.75). Its level is

```math
v = \left\lfloor r \cdot c \cdot g \right\rfloor, \qquad v' = \left\lfloor \frac{127\,v}{100} \right\rfloor, \qquad \text{level} = \frac{v'}{100}
```

  with c the class multiplier and g = G/100 for weapons and default, g = 1 for speech. The products are computed in single precision before truncation (Quirk 6).
- **Heard gain** = master × level. At G = 100 a full-volume weapon sound gets 1.27, a default-class sound 0.95; at G = 50 they get 0.315 and 0.23 (test cases). Whether the sound library lets a level above 1.0 amplify is Open question 9.
- Changing G or the class multipliers updates every playing sound at once.
- **Music** is a separate stream whose master level is M/100 (M = `musicvolume`, clamped 0…100 for storage) when music is on, and 0 when music is off; music off does not pause the stream. When the machine lacks music support, M is forced to 0 at load.
- **Voice:** microphone and speaker volumes go to the voice system as 0…100.

### O. Other tab (social options)

| Shown control | Stored | Values | Applies |
|---|---|---|---|
| Block whispers `blockwhisper_on`, `_off` | account service, option bit "whisper" | on/off | OK (sent to the service when changed) |
| Block invitations `blockinvite_on`, `_off` | account service, "invite" | on/off | OK |
| Chat filter `cursefilter_on`, `_off` | account service, "chat filter" | on/off | OK |
| Spectator mode `spectatormode_on`, `_off` (window `spectatormode_wnd`, label `static_spectatormode`) | account service | on/off; shown only to game masters on the server-select screen | at once (request to the service) |
| Receive notes `recivenote_on`, `_off` | messenger service | from friends / from nobody; disabled while the messenger state is unknown | OK |
| Daily-job popup `jobautopopup_on`, `_off` | account service, "daily quest" bit (stored inverted: bit set = popup off) | on/off | OK |
| Language drop-down `language_combo` (language-pack builds) | `UserLanguage.cfg` in the game's language folder, `[Config] LanguageCode` | a language code | restart (a message box says so) |
| Block list `list_blackUser` (rows `col_BlackUser` with `check_Blackuser`, name `item_name`), buttons `button_regist`, `button_clear`, `sort_name`, `check_all` | a per-account file `<account>_BlackList.txt` in the game's `Etc` folder | names | at once |

- **Defaults** for the Default button come from the profile's `[OtherOption]` section: `Whisper`, `BlockInvite`, `CurseFilter`, `AllowNote`, `DailyQuest` (booleans). Receive notes is only reset while the messenger service is reachable.
- **Block list:** "register" asks for a name in a message box and adds it; "clear" removes every checked name (and reports whether any were checked); "sort" toggles name order ascending/descending (case-insensitive); the header check sets every row's check. Names matching `GM*` can never be blocked.

### H. Options with no page

| Stored | Meaning | Default when missing | Console variable | Where set |
|---|---|---|---|---|
| `[Game] AlwaysRun` | run is the default state, walk key slows (`movement.md`) | 1 | run lock | profile only |
| `[Game] AutoWeaponSwitch` | switch to a new weapon automatically | 1 | `AutoWeaponSwitch` | profile only |
| `[Game] WeaponSway` | weapon bob/sway multiplier ×10 | **0** | `WeaponSway` = value/10 | profile only |
| `[Game] MessageDur` | HUD message duration multiplier ×10 | **0** | `MessageDuration` = value/10 | profile only |
| `[Game] HUDLayout` | HUD layout index | 0 | `HUDLayout` | profile only |
| `[Game] LightMap` | light maps | 1 | `LightMap` | profile only |
| `[Game] DirectSound` | 1 = software sound mixing off | 0 | `DisableHardwareSound` = 1 − value | profile only |
| `[Game] Reality`, `Difficulty`, `SPRadar`, `LoadScreenTips` | single-player leftovers | 0, 2, TRUE, 1 | | profile only |
| `[Display] RadarScale` | radar zoom | 1.0 | | radar zoom keys |
| `[Multiplayer] EnableSavingID`, `SaveID` | remember the login name | FALSE, "" | | login screen |
| `[VideoCapture] Preset`, `VideoFPS`, `CombatarmsLogo` | recording | 2, 30, TRUE | | recording settings |

- **Radar zoom:** while `IncRadarScale` is held (and chat input is closed and the radar is visible), the radar scale grows by 0.01 **per rendered frame**; `DecRadarScale` shrinks it by 0.01 per frame. It is clamped to 0.5…2.0. The scale bar on the radar sits at a height proportional to (2.0 − scale)/1.5 of its area. The value is copied into the profile whenever the radar hides and saved with the profile; on entering a world the radar starts from the stored value.
- **HUD on/off:** each press of `HUDOnOff` flips a flag that starts on at game start and is kept until the game exits (not saved, not reset between matches). Off hides every layout-built HUD panel (health, ammo, kill feed, chat, radar, score panels …). The code-drawn items (crosshair, damage overlays, scope) keep drawing, and so do the scoreboard and popups. A server message can hide the whole HUD independently.
- **Help:** `HelpScreen` toggles the help popup, except for game masters.

### F. Storage, loading and saving

#### F1. Files

| Role | Name | Format | Scope |
|---|---|---|---|
| Player profile | `Profiles\player.txt` under the game folder | key/value text (Units); plaintext in the reference client, encrypted with the archives' profile cipher in the Steam build (Open question 1) | per installation: shared by every Windows user and every game account on that install |
| Default profile (template) | `Profiles\High_Default.prf` (also `Mid_Default.prf`, `Low_Default.prf`, unused) | same | shipped with the game |
| Key names | `KeyMappingString.txt` in the attributes archive | key/value text | shipped |
| Language choice | `UserLanguage.cfg` in the language folder, `[Config] LanguageCode` | ini | per installation |
| Block list | `<account>_BlackList.txt` in the `Etc` folder | key/value text | per account, per installation |
| Music list choices | the music user profile in the `Etc` folder | key/value text | per installation (music and ambience spec) |
| Social options | account service | | per account, any machine |

- The engine's own startup configuration is read but never written back; the profile is the only place settings persist.
- **Profile sections:** `[Display]`, `[Controls]`, `[Bindings]`, `[Game]`, `[Sound]`, `[Multiplayer]`, `[Settings]`, `[Details]`, `[OtherOption]`, `[VideoCapture]`, `[YoutubeUpload]`, `[Version]` (`Major`). The stock profile also carries leftover keys nothing reads (crosshair colours and style, head bob, subtitles, …).

#### F2. Loading order (game start)

1. (Window-mode builds, before the engine starts) Read `[Display]` from `player.txt`: windowed flag, resolution, depth, anti-aliasing and gamma go to the console; an unavailable mode becomes 1024 x 768 x 32.
2. Parse `player.txt`.
   - **Found:** if the default profile has a `[Version]` section and its `Major` differs from the player profile's `Major` (missing = 0), the player profile is **deleted** and loading falls through to "not found".
   - **Not found:** parse the default profile instead and mark this as the first play. If the default profile is missing too, loading fails.
3. In this order: sound (applied), display (applied to the console, including the performance block), controls (K4; bindings and mouse applied), multiplayer, game options (applied), saved login name, recording settings, upload list, `[OtherOption]` defaults, radar scale.
4. Performance: from `[Settings]` if present, otherwise from the current console values; then the preset is applied (G4) and sound is applied again.
5. If the console variable `RestoreDefaults` is 1, every section is reloaded from the default profile and `RestoreDefaults` is set back to 0.

A brand-new profile is written to `player.txt` the first time it is saved.

#### F3. Saving

- The profile is saved when the options screen opens, on OK, when the loadout is saved, when the login name choice changes, after a recording upload and at game exit.
- A save rewrites the whole file: the sections the profile manages are written from the current values (display, bindings and controls, multiplayer, game options, sound, performance, radar scale, login name, recording, uploads); every other line of the parsed file is kept.
- Values written: integers as decimal, floats as decimal, booleans as TRUE/FALSE, binding strings quoted.

#### F4. What the server may override

- Nothing on these pages is pushed by a game server. The client tells the server its environment detail and asset pre-caching (G4). A server message can hide the whole HUD (H). Room rules (friendly fire, kill cam, abilities …) are room options, not player options (`lobby.md`).
- The network bandwidth targets are written as 5,120,000 (client) and 35,840 (server) on every multiplayer apply; the profile's bandwidth keys are stored but ignored.
- The social options of the Other tab are the service's; the client only shows and requests changes.

### L. Layout files

The options screen loads five layout files from the XML archive, in this order, in `ui_system.md`'s format (1:1, never scaled, since menus are always 1024 x 768):

| File | Holds |
|---|---|
| `popup_option.xml` | frame, the four tab buttons, OK, Cancel, close and Default buttons |
| `popup_option_graphic.xml` | `GraphicWnd`: the list `graphic_list` with its scrollbar, the resolution and anti-aliasing drop-downs (row templates `combo_resolution_bar`, `combo_antialias_bar` with `select_element` and `string`), the gamma slider, all `check_*` buttons of G1 and the custom block `detail` |
| `popup_option_sound.xml` | `SoundWnd`: the General and Voice sub-windows, list `List`, music list `list_music` with row template `column_music`, all controls of S1, device row templates `mic_device_info`, `speaker_device_info` |
| `popup_option_control.xml` | `ControlWnd`: the binding list `list_control` (one image row per K1 entry, each with a text `ValueText`), the sensitivity slider and value, the invert and radar-rotation switches |
| `popup_option_other.xml` | `OtherWnd`: list `List`, the switches of O, the block list and its buttons, the optional `language_combo` |

## Per-frame order

1. Window messages: mouse clicks reach the options screen (a click on a binding row starts capture, K3; clicks on buttons fire on mouse down, `ui_system.md` I1).
2. Engine input read: device events are collected; mouse counts become axis offsets (M2); bound keys turn actions on or off.
3. Options screen (Control tab only, while capture waits): read up to 8 recorded events, then K3 steps 2 and 3.
4. Match: commands that went down this frame run (sensitivity keys, HUD toggle, help …); held radar zoom keys change the radar scale by 0.01.
5. Match: camera rotation from the axis offsets (M2), unless a popup blocks it.
6. Render; the options screen draws over the menu.

## Edge cases

- **Out-of-range stored values:** nothing on load clamps sensitivity, gamma, volumes or switches, except: an unavailable display mode becomes 1024 x 768 x 32; the effects master clamps at 0…100 (negative → 100 by the unsigned conversion); a music volume above 100 is stored as 100 but passed to the stream unclamped (Quirk 7). The sliders show the value clamped to the track (W). A sensitivity of 150 gives k = 0.03376 rad/count and is used as is.
- **Missing keys** take the defaults listed in each table. Note the code defaults differ from the stock profile: missing `soundvolume`, `musicvolume`, `WeaponSway` and `MessageDur` are 0, so a profile without them starts silent, with no weapon sway and with zero-length HUD messages.
- **Missing profile file:** created from the default profile (F2). **Missing default profile and missing player profile:** profile loading fails and the game cannot continue past start-up.
- **Template version bump:** a new default profile with a different `[Version] Major` deletes every player's profile on the next start; all settings return to the template.
- **Two actions on one key** (from the file): both are bound and both fire. The fallback rule of K4 can create this by itself (Quirk 1).
- **Capturing a key that is not remappable** (Esc, Pause, F8): capture ends, the action is left with nothing (`N/A`, stored as `" @ @ "`). This is the only way to unbind an action from the UI.
- **Clicking a button while capture waits:** the left mouse button is itself a capturable input, so the click binds the left mouse button to the action being captured (and the button's own click also fires, since buttons act on mouse down).
- **Cancel after Default:** restores the state from when the screen opened, because the profile was saved on opening.
- **Resolution list with 16- and 32-bit modes:** each size appears twice; choosing the 16-bit entry and the 32-bit depth button disagree only until OK, where the depth buttons win for depth.
- **No voice support:** voice values still load and save but nothing applies them.

## Quirks

Keep these unless a reviewer decides otherwise:

1. **Unbinding does not stick for some actions.** `Prev_Weapon`, `SpecialWeapon`, `TripleShotMode` and `TeamChat` get their fallback key back on every start when they have none, even if another action now uses that key. Rebinding Q to another action and restarting leaves Q on both.
2. **Sliders can creep by one.** Showing a stored value v places the thumb at ⌊(v+1)·W/100⌋, which reads back as v + 1 whenever (v+1)·W/100 is whole (always when W is a multiple of 100). The read-back is what OK saves, so opening options and pressing OK can raise sensitivity, gamma or a volume by 1 each time, depending on each slider's track width in the layout (Open question 5).
3. **Gamma slider past 50** gives gamma above 6, up to about 31.5 at 100. The round trip of a stored gamma through the slider is lossy (1.22 shows as 23 and saves as 1.2119).
4. **High preset effects level is 1, not 2,** on shader-capable machines once OK is pressed with a preset selected.
5. **Sensitivity keys have no fallback.** Their fallback keys are listed but never applied, unlike the other listed actions.
6. **Single-precision truncation of sound levels:** at G = 70 a full-volume weapon sound computes 100 × 0.7 as 69.99999…, truncates to 69 and ends at level 0.87 instead of 0.88.
7. **Music volume above 100** in the file is passed to the music stream as is (level > 1).
8. **Radar zoom speed depends on frame rate:** 0.01 per frame, so at 100 fps it takes 1.5 s to go from 0.5 to 2.0, at 30 fps 5 s.
9. **Sensitivity keys stop at 1 and 99,** while the slider reaches 0 and 100.
10. **Effects volume is applied twice** (master and per sound), so the slider is far from linear in loudness.

## Answers to other specs' open questions

- **`ui_system.md` open question 6 (input while the Esc menu is open):** mouse look stops: the camera gets zero mouse offsets on every frame the Esc menu, the user-report box, a message box or the user tooltip is visible (M2). Movement keys are not covered by this spec.
- **`hud_crosshair.md` open question 12 (crosshair toggle):** no option or binding toggles the crosshair alone. The bindable HUD on/off key (`HUDOnOff`, Page Up) hides the layout HUD panels but leaves the crosshair and damage overlays drawn (H).

## Test cases

Common setup unless stated: stock path of M2 (no smoothing); a profile file as described; slider track widths W stated per case; display modes include 1024 x 768 x 32, 1280 x 720 x 32, 1920 x 1080 x 32 and 1920 x 1080 x 16.

**Mouse**

1. S = 5, unzoomed, 1000 counts right in one frame → yaw +1.7025 rad (k = 0.001135).
2. S = 5, zoom level 1, 100 counts right → yaw +0.0425625 rad. Level 2 → +0.0243214 rad.
3. S = 10, unzoomed: counts for 360° = 2π / 0.00339 ≈ 1853.4. S = 1: ≈ 17824.6. S = 100: ≈ 186.1.
4. S = 0 → k = 0.00001; 1000 counts → 0.015 rad.
5. Invert on, 10 counts down at S = 5 → pitch changes by −0.017025 rad instead of +0.017025 rad.
6. Esc menu visible, 500 counts → yaw and pitch unchanged.
7. `SensitivityInc` at S = 5 → S = 6, scale 0.00136, chat line for 5000 ms. At S = 99 → S stays 99, chat line still shown. `SensitivityDec` at S = 1 → stays 1.
8. Profile without `Sensitivity` → S = 5. Profile with `Sensitivity = 150` → k = 0.03376.

**Sliders**

9. W = 100: show v = 5 → thumb at 6 px → value 6 (and OK saves 6). W = 140: show v = 5 → thumb at 8 px → value 5.
10. W = 200: drag the thumb to 99 px → value 49; to 100 px → 50; arrow click from 100 px → 102 px → 51.
11. W = 80: an arrow click moves 1 px. Thumb offsets 0 to 8 read 0, 1, 2, 3, 5, 6, 7, 8, 10: values 4 and 9 cannot be reached.

**Gamma**

12. Slider 0 → 0.5; 4 → 0.75; 16 → 1.0; 33 → 2.25; 50 → 6.0; 100 → 31.519.
13. Stored 1.22 → slider 23 → γ 1.21194. Stored 1.155709 → slider 22 → γ 1.155709. Stored 2.25 → 33.
14. Ramp at γ = 2.0: entry 64 → 32831, entry 128 → 46431, entry 255 → 65535. At γ = 1.22: entry 128 → 37249.

**Sound**

15. G = 100, effects on: weapon sound r = 100 → level 1.27, heard 1.27; default-class r = 100 → 0.95; speech → 1.27.
16. G = 50: weapon 0.63 × 0.5 = 0.315; default 0.46 × 0.5 = 0.23; speech 1.27 × 0.5 = 0.635.
17. G = 70: weapon level 0.87 (not 0.88), heard 0.609; default 0.66, heard 0.462.
18. G = 1: weapon and default levels 0; speech heard 0.0127.
19. Effects off at G = 80 → master 0, everything silent; turning effects on → master 0.8 at once.
20. Stored `soundvolume = -5` → master 1.0.
21. Music on, M = 40 → music stream level 0.4. Music off → 0 (stream not paused). Profile without `musicvolume` → 0.

**Graphics**

22. Low → Mid → OK: console ShellCasings 1, Tracers 1, ShadowDetail 1, projected shadows on with (0, 128, 0, 0), FXDetail 1, client and debris effect levels 1, EnvironmentalDetail 1 (snow density 1.0), DetailLevel 2 (group offsets 1, 1, 2), PerformanceSetting 1.
23. High on a shader-capable machine → OK: Shader 1, FXDetail 2, but client and debris effect levels 1.
24. Low: EnvironmentalDetail 0 → snow density 0.75.
25. Custom, click anisotropic on while trilinear is on → trilinear shows off. Click trilinear on → anisotropic off.
26. Custom with shader on and effects High, click shader off → bloom off and disabled, effects Mid, High disabled.
27. Custom, OK → stored ShadowDetail 0 whatever was there before.
28. Stored 1600 x 900 x 32 not among the modes → loads as 1024 x 768 x 32.
29. Resolution list for the common modes, in order: 1920 x 1080 (32), 1920 x 1080 (16), 1280 x 720, 1024 x 768 (assuming one pixel format).
30. Account age 16 (US build) → blood shows off and is disabled; OK writes Gore 0.

**Bindings**

31. Stock profile, capture on Reload, press Q → `Reload = "##16@ @ "`, `Prev_Weapon = " @ @ "`; after OK and a restart, `Prev_Weapon` is Q again and Q fires both actions.
32. Capture on Jump, press Esc → Jump shows `N/A`, stored `" @ @ "`; after a restart Jump is still unbound.
33. Capture on `PrimaryWeapon`, roll the wheel up → `" @#U@ "` in the mouse slot (the keyboard key is gone too); wheel up selects the primary class.
34. Capture on Fire, press the right mouse button → `Fire = " @##4@ "`, `ZoomIn = " @ @ "`.
35. Capture on Forward, press Up arrow (`##200`) → W is no longer bound; Forward shows the key name of `##200` from the key-name file.
36. Profile with `HUDOnOff = " @ @ "` and Page Up free → on load HUDOnOff = Page Up. Same with Page Up bound to Forward → HUDOnOff stays unbound.
37. Profile with no `HUDOnOff` entry at all and Page Up bound to Forward → the whole `[Bindings]` section is replaced by the default profile's.
38. Profile with `SensitivityInc` missing → stays unbound (no fallback).
39. Keys 1 to 6 always trigger `Choose_1`…`Choose_6` as well as their bound actions after any apply.

**Storage and flow**

40. No `player.txt` → created from the default profile; first-play is set; `Sensitivity` 5 if the template lacks it.
41. `player.txt` with `[Version] Major = 8`, template `Major = 9` → `player.txt` deleted and recreated from the template.
42. Open options, change effects volume 50 → 20 (heard at once), Cancel → 50 again; file unchanged.
43. Open options, Control tab Default, OK → bindings equal the default profile's; `[Sound]` unchanged.
44. Hold `IncRadarScale` for 60 frames from 1.0 → 1.6; for 200 frames → 2.0. Leave the match → `[Display] RadarScale = 1.6` (or 2.0) on the next save.
45. Press `HUDOnOff` → panels hidden, crosshair still drawn; next match → still hidden; restart the game → shown.
46. Radar rotation switched on in the options, OK → the console value changes at once, but the radar only rotates from the next world entry.

## Open questions

1. **Steam profiles are encrypted.** The Steam build's `High_Default.prf` (and `BGMUserProfile.txt`) are not plaintext. The stock keys and the template's values here come from the reference material's plaintext stock profile. Check: in the Steam client, open options, press Default on the Control tab, and compare every row with K1; read the Sound and Graphic defaults the same way.
2. **Template values.** The default profile's `[Sound]`, `[Display]`, `[Controls]` values (volume, gamma, sensitivity) were not readable; only the code fallbacks are certain. The plaintext stock profile has `soundvolume` 5, `musicvolume` 5, `Sensitivity` 10, `ScreenGamma` 1.155709 (slider 22), which may be development values.
3. **Shown labels.** The text of each row, tab and switch is in the options layouts and string table, not read here. Read `popup_option*.xml` and the US string table.
4. **Unlisted actions.** How `TeamState` (scoreboard, Tab in the stock profile), `ClanChat` (F4), `KickUser`, `Choose_7`…`Choose_0` reach the engine is not in the options path traced here (they are not applied from `[Bindings]`). `ui_system.md` and `inventory.md` treat them as fixed keys. Check in game: rebind something to Tab and see whether the scoreboard still opens.
5. **Slider track widths.** Whether the creep of Quirk 2 happens depends on each slider's track width in the layouts. Check live: open options, OK, reopen, and watch the sensitivity and volume numbers.
6. **Window mode.** Whether the Steam client is a window-mode build (Full screen / Window switch present, display changes applied at once). A screenshot of the Graphic tab settles it.
7. **Mouse path.** The reference has two engines; the one used by default in the restored reference skips smoothing (×3 path of M2), while the other default enabled the legacy smoothing path. Which one the Steam client runs is unknown. Check: at S = 5 turn the mouse a known number of counts (a fixed-dpi mouse moved a measured distance) and compare the turn with 1.5·k·counts and with k·counts.
8. **Adaptive mouse.** The client has a console-switched "adaptive mouse" response curve on yaw (off by default, max offset 0.4). No page or profile key sets it; whether the Steam build changes it was not checked.
9. **Levels above 1.** Sound levels reach 1.27 (S2). Whether the sound library clamps them to 1.0 or amplifies is unknown; and whether the music stream is also under the effects master. Check by ear: music loudness at effects volume 0 vs 100.
10. **Receive-notes default.** `[OtherOption] AllowNote` is a boolean while the switch holds a three-way service value (friends, all, nobody, unknown); how TRUE/FALSE map was not traced.
11. **Pitch limit.** The first-person pitch clamp is outside this spec; confirm it belongs to `movement.md`'s eye/camera section.

## Corrections (2026-10-22)

From a comparison pass over the reference's options screen and its
callers against this spec and our implementation
(`games/combat_arms/src/`, file:line relative to that folder). It changes
P "Opening".

1. **The options screen is reached from the lobby's start menu** (P
   "Opening"; `lobby.md` N, `StartButton`).
   - Reference: the lobby's top-bar "menu" button opens the start-menu
     popup, whose option entry opens the options screen as a modal screen
     over whatever lobby screen is current (room list or waiting room).
     Closing it (OK, Cancel, close) returns to that screen unchanged: the
     player's room state is not touched (no un-ready, no *outfitting*
     extra state is sent, unlike the inventory and shop). (A second-password popup can also open it on the Other tab;
     out of scope offline.) There is no way to open it in a match.
   - Ours: the screen opens only with the `options` console command
     (`game/options/mod.rs:40-41`, `game/options/flow.rs:272`); the lobby
     screens have no start menu (`ui/lobby.rs:31-35`), so a player in the
     lobby cannot reach the options.
   - Expect: in the waiting room as a ready non-host, open "menu" → the
     option entry → the options screen shows over the room on the
     Graphic tab; change the sensitivity, press OK → back in the waiting
     room, still ready, the new sensitivity in effect; the other players
     saw no state change. Same from the room list.

Could not determine here: whether the waiting room's start timeout (`lobby.md`
S step 6) survives opening and closing the options during a countdown (the
waiting room re-runs part of its focus handling when the options close).

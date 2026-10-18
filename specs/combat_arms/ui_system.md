# Combat Arms: UI system (layouts, scaling, fonts, strings, screens)

Source basis: the client's widget library and its layout loader, the HUD item base and HUD manager, the in-match chat, scoreboard and in-game menu, the popup and screen managers, the font and string-table loaders and the system font renderer; checked against the XML layouts, font list, string table, HUD layout attributes and client interface attributes of a classic-client data set. The Steam build's encrypted layouts were not read (see Open questions).
Status: draft

Sections: Summary; Units and conventions; Constants; Behavior (L: layout data; S: scaling and anchoring; T: text and fonts; R: string tables; W: widget types; I: input, focus and sounds; M: screens, popups and stacking; H: the screens of a match; D: data versus code); Per-frame order; Edge cases; Quirks; Answers to hud_crosshair.md open questions 2 and 3; Test cases; Open questions.

---

## Summary

- Every screen, popup and HUD panel is a tree of widgets read from XML layout files. The coordinates are reference pixels on a 1024 x 768 canvas, and a child's position is relative to its parent.
- In a match, the HUD and the loading screen are stretched to the real resolution. X and widths are multiplied by W/1024, and Y and heights by H/768, with nothing anchored to an edge. Fonts stretch the same way: the font is rebuilt with its height scaled by H/768 and its average glyph width by W/1024.
- Menus (lobby, shop, result screen) never scale, because the client forces the display to 1024 x 768 whenever a menu screen is shown. Popups opened during a match (the Esc menu, message boxes) do not scale either: they are drawn 1:1 in screen pixels and centred on the real screen.
- Text alignment is relative to the parent widget's box. With centre or right alignment the text's own position on that axis is ignored, which is why aligned texts in the data carry dummy positions.
- A text's displayed string comes from a string table keyed by id. An id that is not found is shown as the raw id, including its leading `$`.
- Widgets are mouse-driven. The topmost widget under the cursor gets the click, and a button fires on mouse *down*, playing its click sound then. Keyboard input reaches only the code of the current screen, popup or HUD panel (Esc, Enter, chat keys). There is no focus ring and no Tab navigation.

## Units and conventions

- **Reference pixels (ref px):** the layout coordinate system. Origin top-left, x to the right, y **down**, integers.
- **Screen pixels (px):** the real back buffer, W x H.
- **Scale factors:** a single pair for the whole HUD, recomputed whenever the display mode changes:

```math
x_r = \frac{W}{1024}, \qquad y_r = \frac{H}{768}
```

  - Menus and in-match popups always use x_r = y_r = 1 (see S1).
- **Pixel coverage of a quad:** the renderer draws every textured quad 0.5 px up and left of its nominal position, to land on pixel centres. A quad whose nominal span is [X, X + Wq) therefore lights the pixel columns

```math
\big[\,\lceil X - \tfrac12 \rceil,\; \lceil X + W_q - \tfrac12 \rceil\,\big)
```

  - Rows work the same way. For integer X and Wq this is just [X, X + Wq). All pixel rects in this spec are half-open [x0, x1) x [y0, y1).
- **Colours:** (R, G, B, A), each 0 to 255.
- **Time:** a millisecond wall clock for fades, blinks, chat and timers. The UI runs once per rendered frame; there is no UI tick.
- **Text encoding:** layouts and string tables are UTF-8 (a byte-order mark is allowed). All displayed text is UTF-16 internally.

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| reference canvas | 1024 x 768 | ref px | layout coordinate space |
| root node size | 1024 x 1024 | ref px | invisible root that every loaded file hangs from (note: 1024 tall, see Quirks) |
| menu display mode | 1024 x 768, 32-bit | px | forced whenever a menu screen gains focus |
| line spacing | font height + floor(font height / 6) | px | distance between wrapped lines |
| stroke offset | 1 (STROKE), 2 (STROKE2X) | px | outline copies, unscaled |
| shadow offset | (+1, +1) (SHADOW), (+2, +2) (SHADOW2X) | px | unscaled |
| default blink | period 1000, hidden 300 | ms | hidden while phase is in (700, 1000) |
| HUD mode fade | 2000 | ms | fade in or out when the HUD switches mode |
| chat line gap | 4 | ref px | between queued chat messages |
| chat message life | 10000 | ms | player, team, whisper and clan lines; system lines use 5000 or 15000 |
| chat history kept | 20 | lines | for chat-record export only |
| chat input max | 119 | characters | layout MaxLength 120, minus 1 |
| round-result banner | 3000 | ms | scoreboard result display after a round |
| result screen timeout | 60000 | ms | auto-return to the room |
| wheel step | raw wheel delta / 12 | units | one notch (120) = 10 |
| button state suffixes | A normal, B hover, C pressed, D disabled | | appended to the button texture prefix |

## Behavior

### L1. Layout files

- **Format:** XML 1.0 in UTF-8 with a BOM and CRLF line ends. The root element is `CBALAYOUT`. It holds `Control` elements, each with a `Type` attribute and one `Property` child whose attributes are the widget's settings. A `Control` may contain further `Control` elements, which are its children.
- **Storage:** the layouts sit in the game's resource archive (see rez_archive.md). In this data set they are plain text; the loader can decrypt, but here it is called without a key.
- **Parent:** set by XML nesting only. The `ParentName` attribute every widget carries is ignored.
- **Loading a file:** a file is loaded under a code-created invisible root node at (0, 0) of size 1024 x 1024, with MouseEvent off. The file's top-level controls become the root's children, in file order.
- **Draw order:** document order. A later sibling draws over an earlier one and wins mouse hit tests.
- **Ignored anchoring keys:** the anchoring keys present on many windows (`RTBenchmarkX/Y`, `RTMoveTypeX/Y`, `RTIncreasingX/Y`, `SameRateIncrease`, always `LEFT/TOP`, `RATE`, `ON`, `OFF` in the data) are never read. There is no anchoring; see S2.
- **Failures:** a file that fails to parse is reported and abandons the rest of that screen's files. A missing file is skipped.
- **Which files load:** the file list per screen, popup and HUD panel is fixed in code (section H).
- **HUD panels as clones:** a HUD panel usually names one top-level control of a shared HUD file. That control is found by name (case-insensitive, depth-first, first match), cloned, and the clone is attached to the panel's own root. A shared HUD file is therefore a library of panels. The loaded copies of the files are thrown away once every panel is built.

### L2. Value syntax

- **Integer pairs (Size, Position, TextureAlphaSize and the like):**
  - Every `.` is replaced by `,`, then the string is split on `,` and each piece is read as an integer, with leading spaces and sign allowed. So "340.80" means (340, 80) and "-7.717" means (-7, 717).
  - Pieces are taken two at a time and the last complete pair wins. A single piece, or an empty string, gives **(1, 1)**. A dangling odd piece is ignored.
- **Colours (Normalcolor, TextureColor, StrokeColor and the like):**
  - Same `.` replacement. Exactly three integers R, G, B are expected, and the result is opaque unless an alpha attribute applies (see W).
  - Three are read at a time; a count that is not a multiple of 3 gives (0, 0, 0, 0), and six give the last triple.
  - Values are not clamped. The data only uses 0 to 255.
- **Float quads (CustomUV, SimpleStretchValue):** split on `,` and read as decimals. The `.` here is a decimal point. Fewer than four values give (0, 0, 0, 0).
- **Booleans (Show, MouseEvent, EnableAlpha, EnableColor, SameSizeParent, AutoSize):** true only for `ON`, ignoring case. Anything else is false.
- **Enumerations (alignments, effects, sounds):** the attribute value is compared, ignoring case, against the *beginning* of each table entry, with the comparison length set by the value. So an exact name matches, a prefix of a name matches the first entry that starts with it, and an empty string matches the first entry. No match gives the entry's default.
- **Type names:**
  - The `Type` value, lowercased, selects which attributes are read. The known types are window, button, simplestretchbutton, checkbutton, image, simplestretchimage, text, buttontypetext, scrollbar, listctrl, scrolllistctrl, columnctrl, combobox, gauge, sliderbar and editctrl.
  - Widget creation then uses the same prefix rule as enumerations.
  - An unknown type is a fatal load error.

### L3. Attributes common to every widget

| Attribute | Meaning | Default when absent |
|---|---|---|
| Name | lookup key for code; need not be unique, first depth-first match wins | (required) |
| Size | width, height in ref px | 0, 0 |
| Position | x, y relative to the parent's position, in ref px | 0, 0 |
| Show | initially visible | ON |
| MouseEvent | takes part in hit tests (see I1) | ON |
| Cmd_id | integer command id that code may read | 0 |
| TextureAlphaSize | the useful (non-padding) texel area of the widget's texture, w x h | 0, 0 |
| Scroller | name of a ScrollBar to bind to: searched among this widget's descendants, then each ancestor's in turn; `none` or empty means none | none |

- **Absolute positions:** a widget's absolute reference position is the sum of its own Position and all its ancestors' positions.
- **Moving a widget:** moving a widget in code moves its whole subtree by the same offset.

### L4. Attributes by type

- **window:**
  - Texture (role: panel background).
  - The texture is drawn like an Image of the window's Size. The placeholder blank texture means "no background".
- **Image:**
  - Texture (role).
  - EnableAlpha with AlphaValue (0 to 255): the quad's alpha.
  - EnableColor with TextureColor: the quad's colour, which multiplies the texture.
  - CustomUV (u0, v0, u1, v1), default all -1: explicit texture coordinates when all four are 0 or more. Otherwise the UVs are computed (W1).
- **SimpleStretchImage:** as Image, plus SimpleStretchValue (u, v, uw, vh), the nine-slice split (W2).
- **Button:**
  - ButtonTexture: a texture-name *prefix* (W3).
  - OverSound: default IS_CHANGE.
  - ClickSound: default IS_SELECT.
  - EventSound: default IS_NONE. It is read into the data but never used.
  - SimpleStretchValue.
- **SimpleStretchButton:** as Button, plus SameSizeParent (default OFF).
- **CheckButton:** as Button, plus CheckButtonType, CHECK or RADIO (anything else means RADIO).
- **Text:**
  - Font: a font-list name (T1).
  - Element: a string id or literal text (R2).
  - HorizontalAlign: DEFAULT, LEFT, RIGHT or HCENTER.
  - VerticalAlign: DEFAULT, TOP, BOTTOM or VCENTER.
  - Effect: DEFAULT, SHADOW, SHADOW2X, STROKE or STROKE2X.
  - Normalcolor, with TextAlphaColor (default 255) as its alpha.
  - StrokeColor (default 0, 0, 0), with StrokeAlphaColor (default 255).
  - ShadowColor (default 0, 0, 0), with ShadowAlphaColor (default 255).
  - WrapWidth in ref px: 0 means no wrapping.
  - A Text has no Size. Its size is measured from its string (T3).
- **ButtonTypeText:** as Text, plus Overcolor, Clickcolor and Disablecolor. These three are always opaque: TextAlphaColor applies only to the normal colour.
- **ListCtrl:**
  - ItemGap: the vertical gap between rows.
  - Blank: space above the first row.
  - BackTexture and BackTextureSimpleStretchValue.
  - AutoSize: grow the background with its rows.
- **ColumnCtrl:** a row template (BackTexture, AutoSize).
- **ScrollBar:** ScrollSize and ScrollPosition in place of Size and Position; bar and button textures; InsertBarGap, BarWidth, WidthScroll; up/down command ids.
- **ScrollListCtrl:** a list plus its own scrollbar.
- **ComboBox:** box and list textures, ItemGap, Blank, ShowListDown, ComboButtonFromListGap.
- **Gauge (W5):**
  - GaugeBackFrameTexture and GaugeBarTexture.
  - GaugeBarPosition: offset of the bar from the frame.
  - GaugeBarMaxRange: the bar's full width in ref px.
  - GaugeBarWidth: the initial fill, in percent.
  - GaugeBarHeight.
  - Optional nine-slice values for the frame and the bar.
- **SliderBar:** frame, bar and left/right button textures with their command ids.
- **EditCtrl:**
  - Font, CaretColor, StringColor.
  - InputOption: normal, numeric or masked.
  - MaxLength, CaretWidth.
  - The layout only places the box. The actual text-entry widget is created by code over the box's rect (I4).

### S1. Which trees scale

- **The scale applied:** every widget carries a screen scale (sx, sy). It is (1, 1) unless code sets it.
- **Scaled to (x_r, y_r):**
  - every HUD panel;
  - the HUD's off-tree row templates (chat line, scoreboard rows), which code rescales itself;
  - the loading screen;
  - tooltips.
  - This happens when the panels are built and again whenever the display mode changes.
- **Always (1, 1):**
  - menu screens: they are only shown at 1024 x 768 (the client switches the display to 1024 x 768 when the first menu screen gains focus, and shuts down if that mode is not available);
  - popups, including those opened during a match.
- **Popup placement:** a popup that centres itself sets its first top-level control to the position below, in screen px. Popups that do not centre stay at their layout position, in screen px.

```math
x = \Big\lfloor \tfrac{W}{2} \Big\rfloor - \Big\lfloor \tfrac{w}{2} \Big\rfloor, \qquad y = \Big\lfloor \tfrac{H}{2} \Big\rfloor - \Big\lfloor \tfrac{h}{2} \Big\rfloor
```

### S2. Geometry of a scaled widget

- **Inputs:** (x, y) is the widget's absolute reference position, (w, h) its reference size, and (sx, sy) its screen scale. σ = (σx, σy) is an optional code-driven "pop" scale, 1 unless an effect sets it (the VIP team-swap flip, for example).
- **Images, windows and nine-slice images:** the nominal quad, about the widget's centre for σ, is

```math
X = x\,s_x + \tfrac{(1-\sigma_x)\,w\,s_x}{2},\quad Y = y\,s_y + \tfrac{(1-\sigma_y)\,h\,s_y}{2},\quad W_q = w\,s_x\,\sigma_x,\quad H_q = h\,s_y\,\sigma_y
```

  - These are floats; nothing is rounded before the pixel-coverage rule.
- **Buttons:** position and size are rounded differently, and σ scales about the screen origin:

```math
X = x\,\sigma_x\,s_x,\qquad Y = y\,\sigma_y\,s_y,\qquad W_q = \lfloor w\,\sigma_x\,s_x \rfloor,\qquad H_q = \lfloor h\,\sigma_y\,s_y \rfloor
```

- **No anchoring:** there is no anchoring and no aspect correction. At 16:9 everything is stretched wider and nothing is pinned to an edge. Content inside 0 to 1024 x 0 to 768 can never leave the screen at any aspect ratio. Content placed outside that range (the data has 1044-wide bars at x = -10) overhangs by a proportional amount.
- **Text** places its top-left corner by truncating its reference position (T4):

```math
X_t = \lfloor x_{text}\,s_x \rfloor, \qquad Y_t = \lfloor y_{text}\,s_y \rfloor
```

### S3. Hit-test rectangles (screen px)

- **The mouse:** screen px.
- **Image, window and button:** these hit the rect below, where (ox, oy) is a code-set pixel offset, normally 0:

```math
\big[\lfloor x\,s_x\rfloor + o_x,\; \lfloor x\,s_x\rfloor + o_x + \lfloor w\,s_x\rfloor\big) \times \big[\lfloor y\,s_y\rfloor + o_y,\; \lfloor y\,s_y\rfloor + o_y + \lfloor h\,s_y\rfloor\big)
```

  - This can differ by 1 px from the drawn rect.
- **Nine-slice image:** uses a width of ⌊w · TextureAlphaSize.x / texture width⌋ when that is nonzero, and likewise for the height.
- **Text:** tests the mouse against its *reference* rect, unscaled. See Quirks.

### T1. Font list

- **Format:** an attribute file of numbered sections SysFont0, SysFont1, ... read until the first missing number. Each section has:
  - Name: the string layouts use, e.g. "bold 13";
  - FileName: a font file installed privately at startup;
  - FaceName;
  - Height in px at 1024 x 768;
  - optional Width (0 = natural), Bold, Italic, Underline, Antialiased.
- **This data set's faces:**
  - a bold face: names "bold 11" to "bold 60";
  - a light face: "Light 11" to "Light 15";
  - a digits face: "number 11" to "number 40".
  - None are synthetic bold; the boldness is in the face. The number in the name equals the Height.
- **Font lookup:** a Text's Font attribute is matched to Name exactly, case-sensitively. A name that is not in the list is a load error, with no fallback.
- **Heights:** the height is the font's cell height. Text measures this many pixels tall at 1024 x 768.

### T2. Font scaling

- **When it is rebuilt:** whenever a Text's screen scale changes to anything but (1, 1), the text requests a new font from the same face:

```math
\text{height} = \lfloor H_f\,s_y \rfloor, \qquad \text{width} = \lfloor a_f\,s_x \rfloor
```

  - H_f is the base font's height in px.
  - a_f is the base font's *average glyph width* in px, a metric of the face at that height (not stored in the data).
- **Forced width:** forcing the average width makes the system stretch every glyph horizontally by about ⌊a_f s_x⌋ / a_f.
- **Net effect:** text scales by about s_x horizontally and s_y vertically, like every other HUD element. At exactly (1, 1) the base font is used again.
- **Effects:** stroke and shadow offsets stay at 1 or 2 px at every resolution.

### T3. Measuring text

- **Unwrapped:** the string is measured with the (scaled) font as one line: width w_px and height h_px, in screen px.
  - The width is the sum of glyph advances (no kerning).
  - The height is the font height.
- **Wrapped:** the width is the widest wrapped line (trailing spaces included). The height is

```math
h_{px} = n\,(h_f + \lfloor h_f/6 \rfloor)
```

  - Here h_f is the scaled font height and n the number of lines.
- **Reference size:** used for alignment, layout by code (the kill feed's running x, list rows, chat stacking) and hit tests:

```math
w = \Big\lfloor \frac{w_{px}}{s_x} \Big\rfloor, \qquad h = \Big\lfloor \frac{h_{px}}{s_y} \Big\rfloor
```

- **Recalculation:** after every text change, font change or scale change.

### T4. Aligning text

- **Inputs:** let (Px, Py) be the parent's absolute reference position and (Pw, Ph) its reference size. For a Text parent, Pw is the wrap width when the text wraps to two or more lines (otherwise its reference width), and Ph is n·h_f + (n − 1)·⌊h_f/6⌋ in raw px, unscaled (the font height for unwrapped text). Let (w, h) be this text's reference size (T3).
- **Default:** with DEFAULT alignment the text's own absolute position is used.
- **Otherwise** the axis is computed from the parent alone, and the text's own Position on that axis is ignored:

| Alignment | Reference coordinate |
|---|---|
| LEFT | x = Px |
| HCENTER | x = Px + ⌊Pw/2⌋ − ⌊w/2⌋ |
| RIGHT | x = Px + Pw − w |
| TOP | y = Py |
| VCENTER | y = Py + ⌊Ph/2⌋ − ⌊h/2⌋ |
| BOTTOM | y = Py + Ph − h |

- **Without a parent:** the parent is the file's invisible root, 1024 x 1024. HCENTER then centres on x = 512, and VCENTER on **y = 512**.
- **To screen:** X_t = ⌊x·s_x⌋, Y_t = ⌊y·s_y⌋.
- **The string image:** drawn with its top-left at (X_t, Y_t), covering [X_t, X_t + w_px) x [Y_t, Y_t + h_px).
- **Wrapped text:** each line is aligned on its own, using that line's width (⌊line px / s_x⌋) in the formula. Line k (0-based) is drawn at Y_t + k(h_f + ⌊h_f/6⌋).

### T5. Wrapping

- **When:** only when WrapWidth > 0.
- **Wrap width in px:** ⌊WrapWidth·s_x⌋, raised to at least the scaled font's average glyph width (or its height, if that width is below 1).
- **Algorithm:** characters are appended one at a time to the current line.
  1. Remember the position just after the most recent space or tab on the current line.
  2. If the line's pixel width now **exceeds** the wrap width, or the character is a newline:
     - If a space was remembered, the line is cut just after that space. The space stays at the end of the upper line, and the rest moves to a new line.
     - With no space on the line, the overflowing character starts a new line. It is dropped instead if it is a space, tab or newline.
     - A newline always ends the line and is not kept.
- **Newline escapes:**
  - A two-character `\n` sequence in an Element, or in text set by code, becomes a real newline only when WrapWidth > 0.
  - In unwrapped text it shows literally as backslash and n.
- **Clipping:** none, by default. Text may overflow its parent box, and nothing truncates or ellipsises it.
  - Code can give a Text a clip rect, which is scaled by (s_x, s_y). Scrolling lists clip their content with a scissor rect (W4).
- **Inline colour runs:** some code paths accept inline colour runs written as `^0xAARRGGBB^text^`. The marker characters are removed and the run is drawn in that colour over the base text. Layout Elements do not use this.

### T6. Text effects and colours

- **Normal colour:** the drawn colour is Normalcolor with alpha TextAlphaColor. A fade (W6) multiplies all three alphas (text, stroke, shadow) by the fade factor.
- **SHADOW / SHADOW2X:** one copy in the shadow colour at (+1, +1) or (+2, +2) px, then the text.
- **STROKE / STROKE2X** (offset e = 1 or 2 px):
  - Unwrapped text: eight copies in the stroke colour at (0, 0), (−e, 0), (+e, 0), (0, −e), (0, +e), (+e, −e), (−e, +e) and (+e, +e), then the text. The upper-left diagonal is missing and one copy sits under the text (quirk).
  - Wrapped text: the copy at (0, 0) is replaced by (−e, −e), so the outline is complete.

### R1. String tables

- **Format:** a text file, UTF-8, with an optional BOM.
  - Lines are split on CR and LF.
  - A line without `=` is ignored, which covers section headers and stray words.
  - The key is the text before `=`, trimmed and lowercased.
  - The value is the text between the first `"` after `=` and the next `"` that is not doubled; a doubled `""` stands for one quote. No quote, or `""`, gives an empty value.
- **Duplicate keys:** reported; the first one wins.
- **Language-pack builds:**
  - The language code comes from a command-line option, else from the code the player saved through the options screen, else "ENG". The saved choice takes effect at the next start.
  - The table read is the language pack's table for that code.
  - An optional per-user override file in the same format can link a value to it. A value written `#key#fallback` looks up `key` in the override file and uses `fallback` if that key is absent.
  - The pack can also map texture names to per-language replacements.
  - Codes seen in one data set: ENG, POR, TUR, DEU, ITA, POL, ARB, SCN.
- **Builds without the pack:** one fixed table is read.
- **No reload:** strings are never reloaded while running.

### R2. Resolving an id

- **Steps:** given an id (an Element value, or a key passed by code):
  1. An empty id resolves to the empty string.
  2. Leading `$` characters are stripped, and the rest is lowercased and looked up.
  3. If a code-set key suffix is active (the shop's rental view), id + suffix is tried first, then the plain id.
- **Not found:** the result is the **original id unchanged**, so `$hud_CHAT` would display as "$hud_CHAT".
- **Plain text is looked up too:** an Element is always looked up, `$` or not. A plain word such as "Eliminate" is replaced if a key of that name exists; otherwise it shows as written. Layouts use this for placeholders such as sample names and "999".
- **Format strings:** a resolved value may hold printf-style fields that code fills in (for example the recording button's key name).

### W1. Image texturing

- **UVs:** unless CustomUV is set, the image shows the top-left w x h texels of its texture: u from 0 to w / T_w and v from 0 to h / T_h, where T_w and T_h are the texture's size in texels. A widget larger than its texture repeats it, since menus draw with wrapping texture addressing.
- **Scaling:** the texels are then stretched to the scaled quad.
- **Missing texture:** the image draws nothing, but it still takes part in hit tests at its Size.

### W2. Nine-slice images and buttons

- **Inputs:** SimpleStretchValue = (u, v, uw, vh) as fractions of the texture.
- **Border sizes, in texels and unscaled:**

```math
b_l = T_w u,\quad b_r = T_w (1 - u - u_w),\quad b_t = T_h v,\quad b_b = T_h (1 - v - v_h)
```

- **Column edges:** X, X + b_l, X + W_q − b_r and X + W_q, each clamped into [X, X + W_q].
- **Row edges:** Y, Y + b_t, Y + H_q − b_b and Y + H_q, **not** clamped.
- **Behaviour:** corners keep their texel size at every resolution while the middle stretches. A box shorter than b_t + b_b overlaps its rows.
- **Common values in the data:** (0.25, 0.25, 0.5, 0.5) and (0.25, 0, 0.5, 1).

### W3. Buttons

- **Textures:** the four state textures are prefix + "A" (normal), "B" (hover), "C" (pressed) and "D" (disabled). A missing B, C or D falls back to A.
  - If A is missing, the button's size becomes 0 x 0: invisible and unclickable.
  - A Size of 0 takes the A texture's size.
- **State:**
  - Hover: B.
  - Hover with the button held: C.
  - Disabled: D.
  - Otherwise A.
  - Code can pin a "selected" look, which shows C.
- **Child texts:** a ButtonTypeText under a button follows the button's state, using Normalcolor, Overcolor, Clickcolor or Disablecolor.
- **Firing:** the button's command fires on **left mouse down** inside it, while it is enabled and its MouseEvent is on. Release fires a separate "up" event, which most screens ignore. Right-down fires a right-click event, and a double-click fires a double-click event.

### W4. Lists and scrolling

- **Row placement:** rows are added in order. Row 0's top is at the list's y + Blank, and each later row's top is at the previous row's top + its height + ItemGap. Rows are usually clones of a ColumnCtrl template defined inside the list in the layout; code detaches the template and clones it per entry.
- **No scroller:** rows past the list's height just continue downward.
- **With a scroller:**
  - The list is clipped to its rect by a scissor scaled by (s_x, s_y).
  - Its content is offset by the scroll position.
  - Rows entirely outside are skipped.

### W5. Gauges

- **Fill:** set as a percentage p, or as value/max.

```math
\text{bar width} = \begin{cases} \text{hidden} & p \le 0 \\ \text{MaxRange} & p \ge 100 \\ \big\lfloor p \cdot \text{MaxRange} / 100 \big\rfloor & \text{otherwise} \end{cases}
```

- **The bar:** a nine-slice image at frame position + GaugeBarPosition, GaugeBarHeight tall. This is the rule behind the HUD's stamina and score bars.

### W6. Fades, blinks and timed motion

- **Fade in over T ms:** shown at once, with alpha ⌊255·t/T⌋ (truncated). At t ≥ T the fade ends and alpha is 255.
- **Fade out over T ms:** alpha 255 − ⌊255·t/T⌋. At t ≥ T the widget is hidden.
- **Scope:** a fade applies to the whole subtree. The HUD uses 2000 ms fades when it switches mode.
- **Blink:** with period P (default 1000 ms) and off time B (default 300 ms), the widget is hidden while ((now − start) mod P) > P − B.
  - A finite count N decreases each time the widget hides. The blink ends on the frame after the count reaches 0, so N gives N − 1 full hidden periods and then a hide of a single frame.
  - −1 blinks forever.
  - Buttons have their own blink: a toggle every given number of seconds that shows the pressed texture.
- **Marquee text:** code can move a text at a speed in ref px/s. It is hidden, and a callback fires, once it leaves its clip rect.

### I1. Mouse hit testing and dispatch

- **Mouse messages:** go first to any modal capture (M2). Otherwise they go through the top-level trees from the last-added (topmost) to the first.
- **Left button down:**
  1. For the first top-level tree whose root contains the cursor, descend: at each level take the last visible child that contains the cursor, until no child does.
  2. Offer the click to that deepest widget, then to its ancestors in turn. Stop at the first that accepts it (buttons accept), and do not offer it to the tree's root itself.
  3. If nothing accepts, try the next tree down.
- **MouseEvent off:** a widget with MouseEvent off never contains the cursor. Its whole subtree is click-through, which is why HUD panels, whose roots have it off, never take clicks.
- **Hidden widgets:** a hidden widget, or one with a hidden ancestor, is never hit. A disabled widget can be hit but does not accept.
- **Release, double-click and wheel:** the button release goes to the widget that took the down event. A double-click goes to that same widget. The wheel goes to the deepest widget under the cursor, then its ancestors, until one with a scroller takes it.
- **Hover:** on mouse move, every visible widget under the cursor gets a hover state, and siblings not under it lose theirs.

### I2. Keyboard

- **No keyboard navigation:** widgets have no keyboard focus, no Tab order and no arrow-key navigation. Those exist only in the legacy menu framework, which the XML screens do not use.
- **Routing, in order:**
  1. **Popups:** visible popups, most recently shown first. The first modal popup stops propagation.
  2. **Match:** in a match, the HUD's key handling (chat when its input is open, otherwise the game's bindings).
  3. **Menus:** in menus, the current screen.
- **Chat input open:** the chat panel consumes every key.
- **Fixed keys in a match:**
  - Esc opens the in-game menu, or closes chat input if that is open.
  - Enter sends chat.
  - F2, F3 and F4 pick all, team or clan chat.
  - F8 toggles voice chat.
  - The scoreboard is visible while its bound key is held.

### I3. Widget sounds

- **Hover sound:** a button plays its OverSound the moment the cursor first enters it. It does not repeat while the cursor stays, and plays again on re-entry.
- **Click sound:** it plays its ClickSound on mouse down, together with firing its command.
- **IS_NONE** plays nothing.
- **Sound ids and what they play** (file named by role in the client interface attributes):

| Id in layouts | Value | Plays |
|---|---|---|
| IS_NONE | 0 | nothing |
| IS_NAVIGATION_CHANGE | 1 | navigation hover |
| IS_NAVIGATION_SELECT | 2 | navigation select |
| IS_ITEM_SELECT | 3 | item select |
| IS_SHOPITEM_EQUIP / _UNEQUIP | 4 / 5 | equip / unequip |
| IS_SILENCER_EQUIP / _UNEQUIP | 6 / 7 | attachment sounds |
| IS_SCOPE_EQUIP / _UNEQUIP | 8 / 9 | attachment sounds |
| IS_MAGAZIN_EQUIP / _UNEQUIP | 10 / 11 | attachment sounds |
| IS_CHANGE | 19 | selection change (the default hover sound) |
| IS_SELECT | 20 | select (the default click sound) |
| IS_PAGE | 21 | page change (empty in this data) |

- **Playback:** sounds play immediately as interface (2D) sounds, and each request plays.
- **Code-only sounds** (not set from layouts): map loading, game start and over, round win and lose, level-up, error notice, unselectable.

### I4. Text entry

- **The box:** an EditCtrl in a layout is only a rect. Code creates one text-entry widget over the rect, with the layout's font, caret colour and string colour, a caret blinking every 0.333 s, and the layout's MaxLength. Its scale is (x_r, y_r) on the HUD.
- **Input options:**
  - Masked entry shows `*` for every character except backspace, tab and enter.
  - Numeric entry accepts digits only.
- **One per tree:** only one entry widget is active per screen or popup tree. A popup's entry widget is shown only while that popup is the topmost visible popup.

### M1. Screens

- **One at a time:** exactly one menu screen is current; screens are not stacked.
- **Switching** to a screen:
  1. Tell the old one it lost focus: its tree is hidden, mouse events off, and its edit box cleared.
  2. Append the new screen's id to a history list.
  3. Build the new screen on first use; the layouts are loaded once and kept.
  4. Give it focus: its tree is shown and mouse events turned on.
- **Back:** "back" goes to the previous id in the history, usually with a fade-out/fade-in transition.
- **Popups:** on any screen change, every visible popup that is marked "close on screen change" is hidden.
- **Menu render order:** normal channel, the modal screen if any, the current screen, the focused channel, modal popups, tooltips, watermark.

### M2. Popups and stacking

- **Building:** popups are registered by id and built (layouts loaded) on first show.
- **Showing a popup:**
  - It moves its tree to the top of the draw order.
  - If modal (the default), it pushes its root onto a capture stack. While the stack is not empty, *all* mouse input goes to the top captured tree, and clicks outside it are swallowed.
  - It updates the cursor.
- **Hiding** removes the popup's root from the capture stack.
- **Order:** the visible popups form an ordered list, newest first, for key routing.
- **Popups in a match:**
  - Drawn after the HUD.
  - Unscaled (S1).
  - While the in-game menu, the user report box, a message box or the user tooltip is visible, the mouse cursor is shown and the scoreboard is suppressed.

### H. The screens of a match

| Screen | Layout data | Opened / closed by | Scale |
|---|---|---|---|
| Loading screen | loading-screen layout | map load start / end | x_r, y_r |
| HUD panels: health block, weapon panel, kill feed, chat, notices, progress messages, captions, radar, item alerts and others | named top-level controls of the main HUD layout (`character`, `weaponRoot`, `KillDeath`, `Chat`, `Notice`, `Progress`, `Caption`, `Radar`, `ItemAlert`, ...) | entering the world; per-panel rules (hud_crosshair.md) | x_r, y_r |
| Mode score panel | mode indicator layout: one `indicator` control with a child per mode; TDM uses `Eli`, and the shared message line is `Etc` | game type known | x_r, y_r |
| Crosshair and damage overlays | drawn by code with HUD layout attributes (hud_crosshair.md) | | x_r (crosshair), x_r, y_r (overlays) |
| Death / kill cam | kill-cam layout, `KillCam` | death / respawn | x_r, y_r |
| Vote kick | vote layout, `VoteKick` | vote messages | x_r, y_r |
| Scoreboard | six scoreboard layouts: common frame `Result_Normal`, team list `Team_UserList`, no-team list, fireteam and two hunted-mode variants | while the score key is held and no in-game popup is up; also forced on for 3000 ms as the round-result banner | x_r, y_r, modal channel |
| Team swap notice (VIP modes) | team-change layout | server team swap | x_r, y_r |
| Chat | `Chat` panel of the main HUD layout | always on; input opened by the chat binds or keys | x_r, y_r |
| In-game menu (Esc) | in-game menu layout | Esc toggles; buttons return to lobby, report a user, start/stop recording, cancel | 1:1, centred |
| Message boxes | message-box layout | code | 1:1, centred |
| End-of-match result | result screen layouts (common, player chart, team, no-team, fireteam, MVP, reward tooltip, abuse notice) | after the match, as a menu screen at 1024 x 768; Enter (when its abuse notice is shown) or 60000 ms returns to the room | none (1024 x 768) |

- **No in-match team, loadout or buy screens:** they do not exist in PvP. Team and loadout are chosen in the waiting room, a menu screen at 1024 x 768. The only in-match shop is a co-op NPC shop button.
- **No pause:** online play has none. The Esc menu does not stop the game.
- **Chat panel (from the layout):**
  - **Queue:** the queue window `ChatQueue` is at (0, 520), 400 x 180.
  - **Line template:** `ChatMsg` at (6, 525), bold 13, white with a stroke at alpha 150, wrap width 380.
  - **Stacking:** each new message is a clone of the template. Messages stack downward from the template's position, each at the previous one's top + its height + 4 ref px.
  - **Overflow:** after adding, while the total height (sum of heights and gaps, minus the last gap) exceeds 180, the oldest message is dropped.
  - **Lifetime:** every frame, for each message older than its life, the *oldest* message is removed (quirk).
  - **Colours** (HUD layout attributes):
    - all-chat from a teammate: (63, 191, 87);
    - all-chat from an enemy: (230, 87, 87);
    - team chat: (196, 196, 196); your own team chat: (255, 255, 255);
    - radio: (107, 142, 242);
    - clan: (35, 206, 255);
    - names starting "GM": (255, 240, 0).
  - **Input bar:** the bar and its tabs sit in the `Active` window, offset (20, 20). Its edit box is at (317, 751), 379 x 19.
- **Scoreboard team list (from the layout):**
  - The panel is at (93, 161), 838 x 432.
  - Alpha rows: list at (164, 192), 766 x 200.
  - Bravo rows: list at (164, 392).
  - Rows are 766 x 25 with gap 0 and blank 0, so row i of a team is at y = 192 + 25i (Alpha) or 392 + 25i (Bravo). Eight rows fill a list.
  - Sorting and column contents belong to the game-modes spec.

### D. What is data and what is code

- **Data (layouts, font list, string tables, attributes):**
  - every position, size, texture, nine-slice split, font name, colour, alpha, initial visibility, static text id, wrap width, alignment, effect, list gap and button sound;
  - the font faces and heights;
  - all displayed strings;
  - HUD colours and timings (kill message life, chat colours and the like, in the HUD layout attributes);
  - the sound file for each sound role (client interface attributes).
- **Code:**
  - the reference resolution and all scaling rules;
  - which files each screen, popup and HUD panel loads, and which named controls it clones and looks up;
  - every dynamic rule: visibility conditions, value formatting, row templates and sorting, the chat queue rules, the scoreboard key, popup centring and modality, the Esc menu and its buttons;
  - screen flow and timeouts;
  - forcing 1024 x 768 for menus;
  - text-effect offsets and the stroke pattern;
  - line spacing;
  - every key binding listed in I2.
- **Consequence:** reading the game's layouts at runtime gives exact geometry and art for any panel we bind by name, but the bindings and all behaviour must be written by us. Rebuilding the layouts as our own data needs only the geometry this spec and hud_crosshair.md give.

## Per-frame order

1. Window messages, as they arrive: mouse to the capture stack or the trees (I1), and keys to popups, then HUD or screen (I2).
2. Update:
   - popups;
   - then the current screen, or the HUD panels in their fixed order;
   - then the widget trees: fades, blinks, button and text state, and text measurement after changes.
3. Render in a match:
   1. world;
   2. code-drawn HUD items (crosshair, damage overlays);
   3. the HUD channel: all HUD panel trees, in panel order;
   4. code-drawn panel extras (the chat edit box);
   5. the modal channel: scoreboard, then popups in show order;
   6. tooltips;
   7. watermark.
4. Render in menus: see M1.

## Edge cases

- **Missing string id:** shows the raw id with its `$`. An id whose value is `""` shows nothing, and its width is 0.
- **Missing font name:** a load error; the layout cannot be built. An implementation should fail loudly at load and may fall back to the first font.
- **Missing button normal texture:** the button collapses to 0 x 0 (W3).
- **Missing image texture:** nothing is drawn, but the image is still hit-testable at its Size.
- **Single-number Position or Size:** reads as (1, 1), not (n, 0) or (0, 0).
- **Colour with 2 or 4 components:** (0, 0, 0, 0), fully transparent.
- **Text with no Normalcolor:** colour 0, which is invisible.
- **Popup larger than the screen:** gets negative coordinates and is cropped. Menus never meet this, since they always run at 1024 x 768.
- **Narrow or wide aspect:** no HUD element goes off screen; everything stretches. Elements authored outside 0 to 1024 overhang proportionally.
- **Odd W or H:** popup centring floors W/2 and H/2 separately from the popup's half size.
- **Text in a box narrower than the text:** a centred text overhangs both sides equally, rounded toward the left. A right-aligned text overhangs to the left. Nothing clips.
- **Scale back to exactly (1, 1):** the base font returns, so widths at 1024 x 768 never depend on rounding.
- **Two controls with the same name:** the first depth-first match is the one code binds.
- **Window texture equal to the placeholder blank texture:** no background quad. Images that use it still draw it (it is transparent).

## Quirks

Keep these unless a reviewer decides otherwise:

1. **The root is 1024 x 1024.** A centred or bottom-aligned text whose parent is the file root centres on y = 512 (bottom 1024), not 384. No shipping layout relies on this.
2. **`.` is a separator** in every integer pair and colour, but a decimal point in UV and nine-slice values.
3. **Anchoring keys are dead data.** The whole HUD stretches non-uniformly; there is no 4:3 pillarbox and no edge pinning.
4. **Double rounding of aligned text.** The text width is truncated to ref px, the position computed in ref px, then truncated again to screen px. At non-4:3 resolutions a right-aligned text can end 1 px past its box (test case 2), and a running-x layout such as the kill feed drifts by up to 1 px per element.
5. **Text hit tests ignore the screen scale,** testing screen-pixel mouse coordinates against reference rects. It only matters on scaled trees, where nothing clickable is text.
6. **Hit rect versus drawn rect** can differ by 1 px (S3 versus S2).
7. **Buttons fire on mouse down,** not on release. Dragging off a button does not cancel.
8. **The unwrapped stroke** misses the upper-left diagonal and draws a stroke copy under the text.
9. **Nine-slice borders do not scale,** and rows are not clamped, while columns are.
10. **Over, click and disabled text colours are always opaque,** whatever TextAlphaColor says.
11. **Chat expiry removes the oldest line,** not the expired one. A 5000 ms system line queued behind a 10000 ms player line removes the player line early, when the system line expires; the system line itself goes on the next frame.
12. **Chat capacity depends on resolution,** because line heights are truncated in ref px: 9 one-line messages at 768 px tall, 10 at 1080 (test cases 9 and 10).
13. **Menus are fixed at 1024 x 768,** and in-match popups are pixel-sized, so the Esc menu looks small on large screens.
14. **Enumerations match by prefix** (L2). A misspelt value that is a prefix of another name silently takes that name, and an empty value takes the first entry.
15. **A finite blink count N** gives N − 1 full hidden periods and then a hide of a single frame (W6).

## Answers to hud_crosshair.md open questions 2 and 3

These are settled here; hud_crosshair.md itself is not changed by this spec.

- **Question 2 (font scaling and widths):**
  - Fonts scale with **both** factors: height ⌊H_f·y_r⌋ px, and average glyph width ⌊a_f·x_r⌋ px, so glyphs stretch horizontally by about x_r. At 1024 x 768 the base font is used.
  - A text's width is measured in screen px with the scaled font and converted to reference units as ⌊w_px / x_r⌋. That reference width drives kill-feed placement: each element is placed at the running x, which then advances by the element's reference width.
  - Stroke and shadow offsets stay at 1 or 2 px.
- **Question 3 (aligned text):**
  - Confirmed: right- and centre-aligned texts align inside their **parent** widget's box, and their own Position is ignored on the aligned axis, so the (100, 100) dummy positions have no effect.
  - The rules are in T4. The weapon name's right edge is at 769 + 250 = 1019 ref.
  - A text with DEFAULT vertical alignment keeps its own y even when centred horizontally.

## Test cases

All test cases use a stub font so they run without the game's fonts:

- **Font "T10":** listed with Height 10, average glyph width a_f = 6 px, monospaced (every glyph advance equals the average width, no kerning), and a line height equal to the font height.
- **Scaled:** its height is ⌊10·y_r⌋ and its advance ⌊6·x_r⌋.
- **Font "T13":** the same with Height 13 and a_f = 8.

The string table holds `t_title = "ABCDE"` and nothing else. Rects are screen px [x0, x1) x [y0, y1).

**Layout L1:**

| Id | Type | Name | Parent | Size | Position | Other attributes |
|---|---|---|---|---|---|---|
| E1 | window | Panel | file root | 200, 100 | 100.50 | Texture = blank placeholder |
| E2 | Image | Bg | E1 | 200, 100 | 0,0 | Texture role: panel background, 256 x 128 texels |
| E3 | Text | Title | E1 | | 100, 100 | Font T10, HCENTER, VCENTER, Element `$t_title`, Normalcolor 255.255.255 |
| E4 | Text | Corner | E1 | | 0,0 | Font T10, RIGHT, BOTTOM, Element `AB`, Normalcolor 255,255,255 |
| E5 | Text | Plain | E1 | | 10.5 | Font T10, DEFAULT, DEFAULT, Element `Hi`, Normalcolor 255,255,255 |
| E6 | Button | Ok | E1 | 50, 20 | 140, 70 | ButtonTexture prefix role: OK button (A, B, C, D all present) |
| E7 | Text | Lonely | file root | | 0,0 | Font T10, HCENTER, VCENTER, Element `XY`, Normalcolor 255,255,255 |

1. **L1 at 1024 x 768** (x_r = y_r = 1):
   - E1: no quad.
   - E2: [100, 300) x [50, 150), UVs u 0 to 200/256, v 0 to 100/128.
   - E3: text "ABCDE", [185, 215) x [95, 105).
   - E4: "AB", [288, 300) x [140, 150).
   - E5: "Hi", [110, 122) x [55, 65).
   - E6: drawn and hit rect [240, 290) x [120, 140).
   - E7: "XY", [506, 518) x [507, 517).
2. **L1 at 1920 x 1080** (x_r = 1.875, y_r = 1.40625; font height 14, advance 11):
   - E2: [187, 562) x [70, 211).
   - E3: 55 x 14 px, reference size 29 x 9, reference position (186, 96). Drawn at [348, 403) x [135, 149).
   - E4: 22 x 14 px, reference size 11 x 9, reference position (289, 141). Drawn at [541, 563) x [198, 212), which overhangs E2 by 1 px right and 1 px down (quirk 4).
   - E5: [206, 228) x [77, 91).
   - E6: drawn [450, 543) x [169, 197); hit rect [450, 543) x [168, 196).
   - E7: reference position (507, 508). Drawn at [950, 972) x [714, 728): centred on y ≈ 721, not the screen centre 540 (quirk 1).
3. **L1 at 800 x 600** (x_r = y_r = 0.78125; font height 7, advance 4):
   - E2: [78, 234) x [39, 117).
   - E3: 20 x 7 px, reference size 25 x 8, reference position (188, 96). Drawn at [146, 166) x [75, 82).
   - E4: 8 x 7 px, reference size 10 x 8, reference position (290, 142). Drawn at [226, 234) x [110, 117).
   - E5: [85, 93) x [42, 49).
   - E6: drawn [187, 226) x [94, 109).
   - E7: [396, 404) x [396, 403).
4. **L1 with E5's Effect set** (1024 x 768):
   - STROKE: stroke copies at (110, 55), (109, 55), (111, 55), (110, 54), (110, 56), (111, 54), (109, 56) and (111, 56), then white text at (110, 55). There is no copy at (109, 54).
   - SHADOW: one shadow copy at (111, 56).
   - STROKE2X: the same pattern with offsets of 2.
5. **Value parsing:**
   - Position "340.80" gives (340, 80).
   - "-7.717" gives (-7, 717).
   - "5" gives (1, 1).
   - "1,2,3" gives (1, 2).
   - Normalcolor "124.190.221" with TextAlphaColor 150 gives (124, 190, 221, 150).
   - TextureColor "1,2" gives (0, 0, 0, 0).
   - "1,2,3,4" gives (0, 0, 0, 0).
   - SimpleStretchValue " 0.25, 0, 0.5, 1" gives (0.25, 0, 0.5, 1).
   - Show "on" is true; "1" is false.
6. **String resolution:**
   - Element `$t_title` gives "ABCDE".
   - `t_title` gives "ABCDE".
   - `$$T_TITLE` gives "ABCDE".
   - `$no_such_id` gives "$no_such_id".
   - `AB` gives "AB".
   - A table line `x = ""` makes `$x` give "" (width 0).
   - A table line without `=` is ignored.
   - With a key suffix "_r" active and `t_title_r = "Rent"` present, `$t_title` gives "Rent".
7. **Wrapping** (T10 at 1024 x 768, WrapWidth 30):
   - "AAA BBB CCC" gives lines "AAA ", "BBB ", "CCC". Reference size 24 x 33 (3·(10 + 1)).
   - "ABCDEFGH" gives "ABCDE", "FGH".
   - WrapWidth 4 is raised to the advance, 6, so "ABC" gives "A", "B", "C".
   - Unwrapped text "A\nB" (backslash and n as two characters) gives one line, "A\nB", 24 px wide.
8. **Wrapped alignment:** "AAA BBB CCC" with WrapWidth 30 and HCENTER under E1.
   - Line 0 ("AAA ", 24 px): x = 100 + 100 − 12 = 188.
   - Line 2 ("CCC", 18 px): x = 200 − 9 = 191.
   - Lines are 11 px apart.
9. **Chat queue at 1024 x 768:** queue window at (0, 520), 400 x 180; template at (6, 525), WrapWidth 380, font T13 (line height 13 + 2 = 15).
   - Ten one-line messages arrive at once. Nine remain, at y = 525 + 19i for i = 0 to 8, and the first is gone.
10. **Chat queue at 1920 x 1080:**
    - Same as case 9 except the font height is 18 and the line is 21 px, so 14 ref; each message takes 18 ref.
    - All ten remain, at y = 525 + 18i.
11. **Chat expiry:** a player line (10000 ms) at t = 0, then a system line (5000 ms) at t = 1000.
    - At t = 6001 the player line is removed and the system line moves up to y = 525.
    - At t = 6002 the system line, also expired, is removed.
12. **Gauge:** MaxRange 120.
    - p = 50 gives width 60.
    - p = 99.9 gives 119.
    - p = 100 gives 120.
    - p = 0 hides the bar.
    - p = −5 hides the bar.
13. **Button textures:**
    - A prefix with A and B present but no C or D: pressed and disabled show A.
    - A prefix with no A: the button is 0 x 0 and never fires.
    - Size 0, 0 with an A texture of 64 x 32 gives a 64 x 32 button.
14. **Button behaviour:**
    - Moving into E6 plays the hover sound once. Moving within it plays nothing more. Leaving and re-entering plays it again.
    - Left-down inside fires the command and plays the click sound before any release.
    - OverSound "IS_NONE" makes hover silent.
    - ClickSound "IS_ITEM_SELECT" plays the item-select sound.
15. **Hit testing:**
    - Two overlapping buttons in the same parent: the later sibling takes the click.
    - The same buttons under a window with MouseEvent OFF: neither takes it, and the click falls to the next top-level tree.
    - A hidden parent makes its children unclickable.
16. **Modal popup:** a modal popup 220 x 145 is shown over a screen with a button outside the popup.
    - Clicking that button does nothing.
    - After the popup is hidden, the same click fires.
17. **In-game menu centring:** the popup's first control is 220 x 145.
    - 1024 x 768: (402, 312).
    - 1920 x 1080: (850, 468).
    - 1366 x 768: (573, 312).
    - It is drawn 1:1 in each case: the background is 220 px wide on screen at every resolution.
18. **Scoreboard rows:** Alpha list at (164, 192), row 766 x 25, gap 0, blank 0.
    - Five Alpha players give rows at y = 192, 217, 242, 267 and 292 (ref).
    - A ninth player's row is at y = 392, overlapping the Bravo list (no clipping).
19. **Fades:** fade-in over 2000 ms.
    - Alpha 63 at 500 ms, 127 at 1000 ms, 255 at 2000 ms (done).
    - Fade-out over 2000 ms: alpha 128 at 1000 ms; hidden at 2000 ms.
20. **Blink:** default blink started at t = 0 with count 2, at 100 fps.
    - Visible on [0, 700], hidden on (700, 1000), visible again on [1000, 1700].
    - Hidden only on the first frame after 1700 (the count reaches 0).
    - Visible from the next frame on, and the blink is over.
21. **Font scaling:**
    - At 1024 x 768, T13 renders "HELLO" 40 x 13 px.
    - At 1920 x 1080 it renders 75 x 18 px (advance ⌊8·1.875⌋ = 15), with reference width ⌊75/1.875⌋ = 40.
    - At 1280 x 1024 it renders 50 x 17 px (advance 10, height ⌊13·1.3333⌋ = 17), with reference width 40.
22. **Missing font:** a Text whose Font is "bold 99" (not in the font list) fails the load with an error naming the control.

## Open questions

1. **Build match:**
   - The layouts read are a classic-client data set stored in plain text.
   - Unverified for the Steam build:
     - that its encrypted layouts have the same structure and values;
     - that it still forces 1024 x 768 for menus (it may offer widescreen lobbies).
   - Check: screenshot the Steam lobby at a widescreen desktop resolution.
2. **String table in non-pack builds:** whether the US client was built with the language pack (code-selected table) or with the single fixed table. This affects only where strings come from, not how ids resolve.
3. **Face metrics:** the real average glyph widths and advances of the bold, light and digits faces were not measured. Pixel-exact text widths need either the user's font files or measured tables.
   - Check: render each font-list entry at 1024 x 768 and record its advances.
4. **Glyph stretch:** how closely a renderer that scales glyph bitmaps by ⌊a_f x_r⌋ / a_f horizontally matches the system's hinted rendering of a font forced to that width. Likely within 1 px per word; worth one screenshot comparison at 1920 x 1080.
5. **Texture addressing:**
   - Menu and popup quads are drawn with wrapping texture addressing.
   - The code-drawn HUD items (crosshair, damage overlays) are drawn with clamping. This bears on hud_crosshair.md open question 5, which predicts clamp.
   - Confirm both with a screenshot.
6. **Input while the Esc menu is open:** whether movement and mouse-look stay active (the cursor is shown, and the game does not pause). Check in a live match.
7. **Mode naming:** that team deathmatch uses the `Eli` child of the mode indicator layout. Confirm against the room's game-type list.
8. **Wrapped-text height mismatch:** the height used as a *parent* box ignores the last line gap, while the text's own reference height includes it (T3 versus T4). No shipping layout puts a child under a wrapped text, so the effect is unobserved.

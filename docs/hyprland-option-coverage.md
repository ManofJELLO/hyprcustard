# Hyprland option coverage

An audit of which Hyprland options hyprcustard lets you set, against
**Hyprland 0.57.2**, the newest release when this was written (2026-11-19).
The source is Hyprland's own list, `hyprctl descriptions -j`: 407 options.

hyprcustard's Look page covers **47** of them. The Animation page covers
**every animation leaf** Hyprland has, so animations have no gaps.

Priorities:

- **P1:** what you see, missing from a page that already exists. Do these first.
- **P2:** what you see, but only some setups use it (window groups, zoom).
- **P3:** how windows behave rather than how they look. This needs a new
  "Behavior" page.
- **P4:** input and hardware. That is a different job from this app's.
- **Leave out:** debugging, expert render switches and toggles for warning
  messages.

Option paths below use the Lua config's dots: `decoration.blur.variant`.

## P1 — the look

*All done 2026-11-19.*

1. **decoration.blur**. *Done 2026-11-19: the Look page now has every blur
   option.* What was missing:
   1. `variant`: kawase (the classic, default), frost, ripple, drops, water,
      fluid_jar, prism, heat_shimmer, acrylic, aurora, haze. Some variants
      are animated and cost GPU time, which the page should say.
   2. Each variant's own settings, shown only while that variant is chosen:
      - `acrylic`: aberration, bulb, clarity, refraction, tint
      - `aurora`: color1, color2, intensity, speed
      - `drops`: speed
      - `fluid_jar`: color, distortion, fill_amount, mass, precision, speed,
        turbulence
      - `glass`, read by frost, drops, prism, heat_shimmer and aurora
        (checked in Hyprland's shaders): refraction, roughness, size
      - `haze`: intensity, iridescence
      - `heat_shimmer`: speed
      - `ripple`: duration, radius, strength, width
      - `water`: damping, duration, radius, speed, strength
   3. `proportional`, `popups_ignorealpha`, `input_methods_ignorealpha`.
2. **decoration.wobble** (8): enabled, intensity, stiffness, damping, mass,
   mesh, and the two settle thresholds (`value_epsilon`, `velocity_epsilon`).
   The thresholds belong under "More".
3. **decoration.motion_blur** (2): enabled, samples.
4. **decoration** (5): `dim_special`, `dim_around` (how much the
   dim-around rule dims; the Look rules page uses that rule), `dim_modal`,
   `border_part_of_window`. Also `screen_shader`, a shader over the whole
   screen, which belongs with the HyprWindowShade pages.
5. **animations.workspace_wraparound** (1).

## P2 — the look, for some setups

1. **group**: the border colors of grouped windows.
   1. `group.col` (4): border_active, border_inactive,
      border_locked_active, border_locked_inactive.
   2. `general.col` (2): nogroup_border, nogroup_border_active.
   3. `group.groupbar` (31) and `group.groupbar.col` (4): the tab bar on a
      group, with its height, fonts, text colors, rounding, gradients, blur
      and indicator. Worth its own card, folded by default.
2. **misc**, the look-related part: `font_family`, `splash_font_family`,
   `animate_manual_resizes`, `animate_mouse_windowdragging`,
   `force_default_wallpaper`, `session_lock_blur`, `session_lock_xray`.
3. **general**: `float_gaps`. Also **layout**: `single_window_aspect_ratio`
   and its tolerance.
4. **cursor**, zoom: `zoom_factor`, `zoom_rigid`, `zoom_detached_camera`,
   `zoom_disable_aa`. Also `hotspot_padding` and `invisible`.

## P3 — behavior (a new page)

1. **general** (behavior): `layout`, `resize_on_border`,
   `extend_border_grab_area`, `hover_icon_on_border`, `resize_corner`,
   `no_focus_fallback`, `modal_parent_blocking`, `allow_tearing`, `locale`.
2. **general.snap** (5): enabled, window_gap, monitor_gap, border_overlap,
   respect_gaps.
3. **The layouts**, each shown only while it is the one in use:
   1. dwindle (11)
   2. master (14)
   3. scrolling (9)
4. **group**, behavior (8): auto_group, insert_after_current, the drag and
   merge switches.
5. **misc**, behavior: swallowing, focus_on_activate,
   initial_workspace_tracking, close_special_on_empty,
   new_float_force_onscreen and float_force_onscreen, middle_click_paste,
   exit_window_retains_fullscreen, on_focus_under_fullscreen,
   mouse_move_focuses_monitor, the DPMS wake switches, vrr,
   render_unfocused_fps, and the ANR dialog.
6. **cursor**, warping: no_warps, persistent_warps,
   warp_on_change_workspace, warp_on_monitor_change, warp_on_toggle_special,
   warp_back_after_non_mouse_input, hide_on_touch, hide_on_tablet.
7. **gestures** (12) and **gestures.scrolling** (2).
8. **binds** (16).
9. **layers** (2).

## P4 — input and hardware

1. **input** (31): keyboard layout, repeat, mouse acceleration and so on.
2. **input.touchpad** (12).
3. **input.tablet** (10), **input.tablettool** (4), **input.touchdevice** (3)
   and **input.virtualkeyboard** (2).
4. **cursor**, hardware: no_hardware_cursors, use_cpu_buffer,
   no_break_fs_vrr, min_refresh_rate, enable_hyprcursor,
   sync_gsettings_theme, default_monitor.

## Leave out

- **debug** (22), **experimental** (1), **quirks** (2), **opengl** (1),
  **input-capture** (2) and **xwayland** (4).
- **render** (19): color management, HDR, scanout and FP16. These are
  expert switches; a wrong one can black out a screen. The exception
  might be `cm_auto_hdr`, if HDR users ask for it.
- **ecosystem** (3), and misc's `disable_*_check` and `*_warning` switches:
  they only silence messages.
- misc's `disable_autoreload`: the app depends on Hyprland reloading the
  file it saves.

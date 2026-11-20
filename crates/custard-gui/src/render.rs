//! The preview renderers behind the GTK and Qt page.
//!
//! Each runs in a child process of this program, started by
//! `custard_core::toolkit_themes::PreviewJob::run` with a flag of its own,
//! and draws a set of sample widgets offscreen to a PNG. Nothing is shown.

use std::path::Path;
use std::time::Duration;

/// The Qt one: C++, in `qt_preview.cpp`, since Qt Widgets has no Rust
/// binding here.
mod qt {
    use std::ffi::{c_char, c_int};
    extern "C" {
        pub fn custard_render_qt_preview(
            out: *const c_char,
            style: *const c_char,
            font: *const c_char,
        ) -> c_int;
    }
}

/// Draw the Qt sample with `style` (Kvantum's theme is in
/// `$XDG_CONFIG_HOME/Kvantum`), in `font` when it is not empty.
pub fn render_qt(out: &Path, style: &str, font: &str) -> Result<(), String> {
    use std::ffi::CString;
    let c = |s: &str| CString::new(s).map_err(|e| e.to_string());
    let (out, style, font) = (c(&out.to_string_lossy())?, c(style)?, c(font)?);
    // SAFETY: three NUL-terminated strings that outlive the call; the
    // function makes its own QApplication, and none exists in this process.
    let code =
        unsafe { qt::custard_render_qt_preview(out.as_ptr(), style.as_ptr(), font.as_ptr()) };
    match code {
        0 => Ok(()),
        2 => Err(format!("Qt has no style called {}", style.to_string_lossy())),
        _ => Err("could not save the preview".into()),
    }
}

/// The widgets the GTK preview shows, matching the Qt one.
// GTK 3's builder crashes on an object written inside a property, so the
// slider's adjustment is one of its own, referred to by id.
const GTK_SAMPLE: &str = r#"<interface>
<object class="GtkAdjustment" id="level">
 <property name="upper">100</property>
 <property name="value">60</property>
</object>
<object class="GtkOffscreenWindow" id="window">
 <property name="default-width">520</property>
 <property name="default-height">300</property>
 <child><object class="GtkBox">
  <property name="spacing">10</property>
  <property name="border-width">10</property>
  <child><object class="GtkNotebook">
   <property name="hexpand">True</property>
   <child><object class="GtkBox">
    <property name="orientation">vertical</property>
    <property name="spacing">8</property>
    <property name="border-width">10</property>
    <child><object class="GtkBox">
     <property name="spacing">6</property>
     <property name="homogeneous">True</property>
     <child><object class="GtkButton"><property name="label">Button</property></object></child>
     <child><object class="GtkButton"><property name="label">Default</property>
      <style><class name="suggested-action"/></style></object></child>
    </object></child>
    <child><object class="GtkCheckButton"><property name="label">Check box</property><property name="active">True</property></object></child>
    <child><object class="GtkRadioButton"><property name="label">Radio button</property></object></child>
    <child><object class="GtkEntry"><property name="text">Text field</property></object></child>
    <child><object class="GtkComboBoxText" id="combo"><items><item>Combo box</item></items></object></child>
    <child><object class="GtkScale">
     <property name="draw-value">False</property>
     <property name="adjustment">level</property>
    </object></child>
    <child><object class="GtkProgressBar"><property name="fraction">0.45</property></object></child>
   </object></child>
   <child type="tab"><object class="GtkLabel"><property name="label">General</property></object></child>
   <child><object class="GtkBox"/></child>
   <child type="tab"><object class="GtkLabel"><property name="label">Advanced</property></object></child>
  </object></child>
  <child><object class="GtkScrolledWindow">
   <property name="width-request">240</property>
   <property name="shadow-type">in</property>
   <child><object class="GtkListBox" id="list">
    <child><object class="GtkLabel"><property name="label">First item</property><property name="xalign">0</property><property name="margin">6</property></object></child>
    <child><object class="GtkLabel"><property name="label">Selected item</property><property name="xalign">0</property><property name="margin">6</property></object></child>
    <child><object class="GtkLabel"><property name="label">Third item</property><property name="xalign">0</property><property name="margin">6</property></object></child>
   </object></child>
  </object></child>
 </object></child>
</object>
</interface>
"#;

/// Draw [`GTK_SAMPLE`] with the theme in `GTK_THEME` and save it as a PNG.
/// GTK 3 is loaded here, at run time, so the app does not depend on it.
/// Run only in the child process `PreviewJob::run` starts.
pub fn render_gtk(out: &Path) -> std::result::Result<(), String> {
    use std::ffi::{c_char, c_int, c_void, CString};
    type P = *mut c_void;

    let out = CString::new(out.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let ui = CString::new(GTK_SAMPLE).map_err(|e| e.to_string())?;
    // SAFETY: each symbol is looked up in GTK 3 by name and called with the
    // signature GTK 3 declares for it; every pointer passed is one GTK
    // returned, or a NUL-terminated string that outlives the call.
    unsafe {
        let lib = libc::dlopen(c"libgtk-3.so.0".as_ptr(), libc::RTLD_NOW);
        if lib.is_null() {
            return Err("GTK 3 is not installed".into());
        }
        macro_rules! sym {
            ($name:literal, $ty:ty) => {{
                let p = libc::dlsym(lib, concat!($name, "\0").as_ptr().cast());
                if p.is_null() {
                    return Err(format!("GTK 3 has no {}", $name));
                }
                std::mem::transmute::<*mut c_void, $ty>(p)
            }};
        }
        let init_check = sym!("gtk_init_check", unsafe extern "C" fn(*mut c_int, P) -> c_int);
        let builder_new = sym!("gtk_builder_new", unsafe extern "C" fn() -> P);
        let add_from_string = sym!(
            "gtk_builder_add_from_string",
            unsafe extern "C" fn(P, *const c_char, isize, *mut P) -> u32
        );
        let get_object =
            sym!("gtk_builder_get_object", unsafe extern "C" fn(P, *const c_char) -> P);
        let show_all = sym!("gtk_widget_show_all", unsafe extern "C" fn(P));
        let set_active = sym!("gtk_combo_box_set_active", unsafe extern "C" fn(P, c_int));
        let row_at = sym!("gtk_list_box_get_row_at_index", unsafe extern "C" fn(P, c_int) -> P);
        let select_row = sym!("gtk_list_box_select_row", unsafe extern "C" fn(P, P));
        let pending = sym!("gtk_events_pending", unsafe extern "C" fn() -> c_int);
        let iterate = sym!("gtk_main_iteration_do", unsafe extern "C" fn(c_int) -> c_int);
        let pixbuf = sym!("gtk_offscreen_window_get_pixbuf", unsafe extern "C" fn(P) -> P);
        let save = sym!(
            "gdk_pixbuf_savev",
            unsafe extern "C" fn(P, *const c_char, *const c_char, P, P, *mut P) -> c_int
        );

        if init_check(std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
            return Err("GTK could not open the display".into());
        }
        let builder = builder_new();
        let mut error: P = std::ptr::null_mut();
        if add_from_string(builder, ui.as_ptr(), -1, &mut error) == 0 {
            return Err(format!("the sample did not build: {}", gerror_message(error)));
        }
        let window = get_object(builder, c"window".as_ptr());
        if window.is_null() {
            return Err("the sample did not build".into());
        }
        set_active(get_object(builder, c"combo".as_ptr()), 0);
        let list = get_object(builder, c"list".as_ptr());
        select_row(list, row_at(list, 1));
        show_all(window);
        // Let it lay out and draw: a few rounds, since drawing comes on the
        // frame clock after the events that ask for it.
        for _ in 0..20 {
            while pending() != 0 {
                iterate(0);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let image = pixbuf(window);
        if image.is_null() {
            return Err("GTK drew nothing".into());
        }
        if save(
            image,
            out.as_ptr(),
            c"png".as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut error,
        ) == 0
        {
            return Err(format!("could not save the preview: {}", gerror_message(error)));
        }
    }
    Ok(())
}

/// A GError's message: `{ GQuark domain; gint code; gchar *message; }`.
///
/// # Safety
/// `error` is null or a GError GLib filled in.
unsafe fn gerror_message(error: *mut std::ffi::c_void) -> String {
    if error.is_null() {
        return "no reason given".into();
    }
    #[repr(C)]
    struct GError {
        domain: u32,
        code: i32,
        message: *const std::ffi::c_char,
    }
    // SAFETY: as the caller promises.
    let e = unsafe { &*(error as *const GError) };
    if e.message.is_null() {
        return "no reason given".into();
    }
    // SAFETY: GLib's messages are NUL-terminated strings.
    unsafe { std::ffi::CStr::from_ptr(e.message) }.to_string_lossy().into_owned()
}

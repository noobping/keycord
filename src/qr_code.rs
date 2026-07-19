use crate::i18n::gettext;
use crate::support::ui::{dialog_content_shell, flat_icon_button_with_tooltip};
use adw::glib::Bytes;
use adw::gtk::{gdk, Align, Box as GtkBox, Button, Orientation, Picture, Widget};
use adw::prelude::*;
use adw::{Dialog, Toast, ToastOverlay};
use qrcode::types::Color;
use qrcode::{EcLevel, QrCode};
use std::rc::Rc;

const QR_BUTTON_ICON_NAME: &str = "qr-code-symbolic";
const QR_QUIET_ZONE_MODULES: usize = 4;
const QR_TARGET_TEXTURE_SIZE: usize = 384;
const QR_MIN_MODULE_SIZE: usize = 2;

struct QrRgbaImage {
    size: usize,
    pixels: Vec<u8>,
}

fn qr_rgba_image(text: &str) -> Result<QrRgbaImage, qrcode::types::QrError> {
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::L)?;
    let code_size = code.width();
    let total_modules = code_size + QR_QUIET_ZONE_MODULES * 2;
    let module_size = (QR_TARGET_TEXTURE_SIZE / total_modules).max(QR_MIN_MODULE_SIZE);
    let size = total_modules * module_size;
    let mut pixels = vec![255; size * size * 4];

    for (index, color) in code.to_colors().into_iter().enumerate() {
        if color != Color::Dark {
            continue;
        }

        let module_x = index % code_size + QR_QUIET_ZONE_MODULES;
        let module_y = index / code_size + QR_QUIET_ZONE_MODULES;
        for y in module_y * module_size..(module_y + 1) * module_size {
            for x in module_x * module_size..(module_x + 1) * module_size {
                let offset = (y * size + x) * 4;
                pixels[offset..offset + 3].fill(0);
            }
        }
    }

    Ok(QrRgbaImage { size, pixels })
}

fn qr_texture(text: &str) -> Result<gdk::MemoryTexture, qrcode::types::QrError> {
    let image = qr_rgba_image(text)?;
    let bytes = Bytes::from_owned(image.pixels);
    Ok(gdk::MemoryTexture::new(
        image.size as i32,
        image.size as i32,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        image.size * 4,
    ))
}

pub fn show_qr_code(text: &str, overlay: &ToastOverlay, parent: &impl IsA<Widget>) {
    let texture = match qr_texture(text) {
        Ok(texture) => texture,
        Err(_) => {
            overlay.add_toast(Toast::new(&gettext(
                "This value is too long for a QR code.",
            )));
            return;
        }
    };

    let picture = Picture::for_paintable(&texture);
    picture.set_halign(Align::Center);
    picture.set_valign(Align::Center);
    picture.set_can_shrink(true);
    picture.set_margin_top(18);
    picture.set_margin_bottom(18);
    picture.set_margin_start(18);
    picture.set_margin_end(18);

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&picture);

    let dialog = Dialog::builder()
        .title(gettext("QR code"))
        .content_width(440)
        .content_height(480)
        .follows_content_size(true)
        .child(&dialog_content_shell(
            "QR code",
            Some("Scan to use this value on another device."),
            &content,
        ))
        .build();
    dialog.present(Some(parent));
}

pub fn connect_qr_button<F>(button: &Button, overlay: &ToastOverlay, text: F)
where
    F: Fn() -> String + 'static,
{
    let overlay = overlay.clone();
    let parent = button.clone();
    button.connect_clicked(move |_| show_qr_code(&text(), &overlay, &parent));
}

pub fn copy_qr_button_group(copy_button: &Button, qr_tooltip: &str) -> (GtkBox, Button) {
    let qr_button = flat_icon_button_with_tooltip(QR_BUTTON_ICON_NAME, qr_tooltip);
    let group = GtkBox::new(Orientation::Horizontal, 0);
    group.set_valign(Align::Center);
    group.add_css_class("linked");
    group.append(copy_button);
    group.append(&qr_button);
    (group, qr_button)
}

pub fn connect_copy_and_qr_buttons<F>(
    copy_button: &Button,
    qr_button: &Button,
    overlay: &ToastOverlay,
    text: F,
) where
    F: Fn() -> String + 'static,
{
    let text: Rc<dyn Fn() -> String> = Rc::new(text);
    let copy_text = text.clone();
    crate::clipboard::connect_copy_button(copy_button, overlay, move || copy_text());
    connect_qr_button(qr_button, overlay, move || text());
}

#[cfg(test)]
mod tests {
    use super::{qr_rgba_image, QR_QUIET_ZONE_MODULES};

    #[test]
    fn rendered_qr_code_has_white_quiet_zone_and_opaque_pixels() {
        let image = qr_rgba_image("correct horse battery staple").expect("QR code");
        assert!(image.size > QR_QUIET_ZONE_MODULES * 2);
        assert!(image.pixels.chunks_exact(4).all(|pixel| pixel[3] == 255));
        assert!(image.pixels[..image.size * 4]
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 255, 255, 255]));
        assert!(image
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 0, 255]));
    }

    #[test]
    fn oversized_value_is_rejected() {
        let text = "x".repeat(4_000);
        assert!(qr_rgba_image(&text).is_err());
    }
}

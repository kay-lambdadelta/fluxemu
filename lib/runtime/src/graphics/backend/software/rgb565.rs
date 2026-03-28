use palette::{Srgb, cast::ComponentOrder};

pub struct Rgb565Order;

impl ComponentOrder<Srgb<u8>, [u8; 2]> for Rgb565Order {
    fn pack(color: Srgb<u8>) -> [u8; 2] {
        let red = (color.red >> 3) as u16;
        let green = (color.green >> 2) as u16;
        let blue = (color.blue >> 3) as u16;

        let packed = (red << 11) | (green << 5) | blue;

        packed.to_be_bytes()
    }

    fn unpack(packed: [u8; 2]) -> Srgb<u8> {
        let packed = u16::from_be_bytes(packed);

        let red = ((packed >> 11) & 0b00011111) as u8;
        let green = ((packed >> 5) & 0b00111111) as u8;
        let blue = (packed & 0b00011111) as u8;

        Srgb::new(
            (red << 3) | (red >> 2),
            (green << 2) | (green >> 4),
            (blue << 3) | (blue >> 2),
        )
    }
}

pub struct Bgr565Order;

impl ComponentOrder<Srgb<u8>, [u8; 2]> for Bgr565Order {
    fn pack(color: Srgb<u8>) -> [u8; 2] {
        let red = (color.red >> 3) as u16;
        let green = (color.green >> 2) as u16;
        let blue = (color.blue >> 3) as u16;

        let packed = (blue << 11) | (green << 5) | red;

        packed.to_be_bytes()
    }

    fn unpack(packed: [u8; 2]) -> Srgb<u8> {
        let packed = u16::from_be_bytes(packed);

        let blue = ((packed >> 11) & 0b00011111) as u8;
        let green = ((packed >> 5) & 0b00111111) as u8;
        let red = (packed & 0b00011111) as u8;

        Srgb::new(
            (red << 3) | (red >> 2),
            (green << 2) | (green >> 4),
            (blue << 3) | (blue >> 2),
        )
    }
}

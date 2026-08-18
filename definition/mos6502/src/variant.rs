use core::fmt::Debug;

use crate::instruction::AddressingMode;

pub trait Variant: Send + Sync + Debug + 'static {
    const SUPPORTS_DECIMAL: bool;
    const SUPPORTS_INTERRUPTS: bool;
    const HAS_ABSOLUTE_INDIRECT_PAGE_WRAP_ERRATA: bool;
    const WDC_VARIANT: bool;

    fn is_addressing_mode_valid(mode: &AddressingMode) -> bool;
}

#[macro_export]
macro_rules! define_variant {
    (
        $name:ident,
        $supports_decimal:expr,
        $supports_interrupts:expr,
        $has_absolute_indirect_page_wrap_errata:expr,
        $wdc_variant:expr
    ) => {
        #[derive(Debug)]
        pub struct $name;

        impl Variant for $name {
            const SUPPORTS_DECIMAL: bool = $supports_decimal;
            const SUPPORTS_INTERRUPTS: bool = $supports_interrupts;
            const HAS_ABSOLUTE_INDIRECT_PAGE_WRAP_ERRATA: bool =
                $has_absolute_indirect_page_wrap_errata;
            const WDC_VARIANT: bool = $wdc_variant;

            fn is_addressing_mode_valid(mode: &AddressingMode) -> bool {
                if let AddressingMode::Mos6502(_) = mode {
                    return true;
                }

                if let AddressingMode::Wdc65C02(_) = mode
                    && Self::WDC_VARIANT
                {
                    return true;
                }

                false
            }
        }
    };
}

define_variant!(Mos6502, true, true, true, false);
define_variant!(Mos6507, true, false, true, false);
define_variant!(Ricoh2A0x, false, true, true, false);
define_variant!(Wdc65C02, true, true, false, true);

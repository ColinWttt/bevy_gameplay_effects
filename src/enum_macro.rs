#[macro_export]
macro_rules! stats {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[repr(u8)]
        #[derive(Copy, Clone, Eq, PartialEq, Debug)]
        pub enum $name {
            $($variant,)*
            None = 255
        }

        impl From<$name> for u8 {
            fn from(e: $name) -> Self {
                e as u8
            }
        }

        impl $name {
            pub fn from_u8(value: u8) -> Self {
                match value {
                    $(x if x == $name::$variant as u8 => $name::$variant,)*
                    _ => panic!("Invalid value {} for enum {}", value, stringify!($name)),
                }
            }
        }

        impl $crate::prelude::StatTrait for $name {
            const NONE: Self = $name::None;

            fn variants() -> &'static [Self] {
                &[$(Self::$variant),*]
            }
        }

        // Array holding all variants
        //pub const $name_VARIANTS: &[$name] = &[$($name::$variant),*];
    };
}

//! [`named_enum!`]: unit enums named by lower-case words (spec tokens and option values).

/// Declares a unit enum named by lower-case words: `ALL`, `name`, case-insensitive
/// `FromStr`, `Display`, and serde as the name.
macro_rules! named_enum {
    ($(#[$meta:meta])* pub enum $name:ident ($what:literal) {
        $($(#[$vmeta:meta])* $var:ident = $s:literal,)*
    }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name { $($(#[$vmeta])* $var,)* }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$var,)*];

            /// The lower-case name used in specs, options and configuration.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self { $(Self::$var => $s,)* }
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|v| v.name().eq_ignore_ascii_case(s))
                    .ok_or_else(|| format!(concat!("unknown ", $what, " {:?}"), s))
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.name())
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.name())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = <String as ::serde::Deserialize>::deserialize(d)?;
                s.parse().map_err(::serde::de::Error::custom)
            }
        }
    };
}

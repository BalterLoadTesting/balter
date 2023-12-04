#![allow(unused_macros)]
/// Helper macro for appending feature tags to docs.

macro_rules! cfg_rt {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "rt")]
            #[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
            $item
        )*
    }
}

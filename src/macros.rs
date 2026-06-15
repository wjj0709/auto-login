macro_rules! impl_ref_accessors {
    ($type:ty { $($field:ident : $field_type:ty => $getter:ident, $setter:ident;)+ }) => {
        impl $type {
            $(
                #[allow(dead_code)]
                pub fn $getter(&self) -> &$field_type {
                    &self.$field
                }

                #[allow(dead_code)]
                pub fn $setter(&mut self, value: $field_type) -> &mut Self {
                    self.$field = value;
                    self
                }
            )+
        }
    };
}

macro_rules! impl_copy_accessors {
    ($type:ty { $($field:ident : $field_type:ty => $getter:ident, $setter:ident;)+ }) => {
        impl $type {
            $(
                #[allow(dead_code)]
                pub fn $getter(&self) -> $field_type {
                    self.$field
                }

                #[allow(dead_code)]
                pub fn $setter(&mut self, value: $field_type) -> &mut Self {
                    self.$field = value;
                    self
                }
            )+
        }
    };
}

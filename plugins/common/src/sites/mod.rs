pub mod cse;
pub mod eco;
pub mod sec;
pub mod sw;

pub use cse::bachelor::CseBachelorPlugin;
pub use cse::employment::CseEmploymentPlugin;
pub use cse::graduate::CseGraduatePlugin;
pub use eco::EcoPlugin;
pub use sec::SecPlugin;
pub use sw::bachelor::SwBachelorPlugin;
pub use sw::graduate::SwGraduatePlugin;

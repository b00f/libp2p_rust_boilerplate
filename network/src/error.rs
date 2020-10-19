


pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),

    BlockRequestError{reason: String},

    Other,
}
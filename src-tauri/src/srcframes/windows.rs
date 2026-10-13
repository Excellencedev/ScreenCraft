//! Source frames on Windows. Not implemented yet: every request gets a 404.

use super::Request;

pub fn submit(req: Request) {
    (req.reply)(None);
}

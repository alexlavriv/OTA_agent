#[derive(PartialEq)]
enum Color {
    _Black,
    Red,
    Green,
    Yellow,
    Blue,
    Purple,
    Cyan,
    White,
    None
}

fn get_color_code(color: Color, bold: bool) -> &'static str {
    if bold {
        match color {
            Color::Red => "9",
            Color::Green => "10",
            Color::Yellow => "11",
            Color::Blue => "12",
            Color::Purple => "13",
            Color::Cyan => "14",
            Color::White => "15",
            _ => "0",
        }
    } else {
        match color {
            Color::Red => "1",
            Color::Green => "2",
            Color::Yellow => "3",
            Color::Blue => "4",
            Color::Purple => "5",
            Color::Cyan => "6",
            Color::White => "7",
            _ => "0",
        }
    }
}

fn get_color(value: &str, bold: bool, color: Color, background: Color) -> String {
    // Get string with console color information.
    let mut result = String::from("\u{1b}[");
    // [1;] means bold.
    if bold {
        result.push('1');
    }
    // Handle foreground.
    if color != Color::None {
        result.push(';');
        result.push_str("38;5;"); // Codes for ANSI foreground.
        result.push_str(get_color_code(color, bold));
    }
    // Handle background.
    if background != Color::None {
        result.push(';');
        result.push_str("48;5;"); // Codes for ANSI background.
        result.push_str(get_color_code(background, bold));
    }
    result.push('m'); // End token.
    result.push_str(value);
    result.push_str("\u{1b}[0m");
    result
}

pub trait Coloralex {
    fn red(self, bold: bool) -> String;
    fn green(self, bold: bool) -> String;
    fn yellow(self, bold: bool) -> String;
    fn blue(self, bold: bool) -> String;
    fn purple(self, bold: bool) -> String;
    fn cyan(self, bold: bool) -> String;
    fn white(self, bold: bool) -> String;
    fn none(self, bold: bool) -> String;
}

impl Coloralex for &str {
    fn red(self, bold: bool) -> String {
        get_color(self, bold, Color::Red, Color::None)
    }
    fn green(self, bold: bool) -> String {
        get_color(self, bold, Color::Green, Color::None)
    }
    fn yellow(self, bold: bool) -> String {
        get_color(self, bold, Color::Yellow, Color::None)
    }
    fn blue(self, bold: bool) -> String {
        get_color(self, bold, Color::Blue, Color::None)
    }
    fn purple(self, bold: bool) -> String {
        get_color(self, bold, Color::Purple, Color::None)
    }
    fn cyan(self, bold: bool) -> String {
        get_color(self, bold, Color::Cyan, Color::None)
    }
    fn white(self, bold: bool) -> String {
        get_color(self, bold, Color::White, Color::None)
    }
    fn none(self, bold: bool) -> String {
        get_color(self, bold, Color::White, Color::None)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn test_colorize() {
        use colored::Colorize;
        for r in 0..255 {
            if r % 50 == 0 {
                for g in 0..255 {
                    if g % 50 == 0 {
                        for b in 0..255 {
                            if b % 50 == 0 {
                                print!(
                                    "{}",
                                    format!("[{:3} {:3} {:3}] ", r, g, b)
                                        .truecolor(r, g, b)
                                        .bold()
                                );
                            }
                        }
                        println!();
                    }
                }
            }
        }
        println!();
        println!(
            "{}, {}, {}, {}",
            "Yellow (BOLD)".to_string().yellow().bold(),
            "[230, 190, 0]".to_string().truecolor(230, 190, 0).bold(),
            "[250, 240, 165]"
                .to_string()
                .truecolor(250, 240, 165)
                .bold(),
            "[250, 250, 0]".to_string().truecolor(250, 250, 0).bold(),
        );
        println!(
            "{}, {}, {}, {}",
            "Yellow".to_string().yellow(),
            "Yellow (BOLD)".to_string().yellow().bold(),
            "Cyan".to_string().cyan(),
            "Cyan (BOLD)".to_string().cyan().bold(),
        );
    }

    #[test]
    #[ignore]
    fn test_colors() {
        use crate::utils::color::Coloralex;
        println!(
            "{}, {}, {}, {}",
            "Yellow".to_string().yellow(false),
            "Yellow (BOLD)".to_string().yellow(true),
            "Cyan".to_string().cyan(false),
            "Cyan (BOLD)".to_string().cyan(true),
        );
    }
}

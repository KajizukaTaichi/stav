use clap::Parser;
use std::{
    fs::{File, read_to_string},
    io::Write,
    path::Path,
};

#[derive(Parser)]
#[command(name = "StaV", about = "Stack-based composition system")]
struct Cli {
    /// Source code file path
    path: String,
}

fn main() {
    let cli = Cli::parse();
    let filename = Path::new(&cli.path);

    macro_rules! fault {
        ($msg: literal) => {
            eprintln!("Failed to {}", $msg);
            return;
        };
    }

    let Ok(source) = read_to_string(filename) else {
        fault!("read source file");
    };
    let Some(html) = stav(&source) else {
        fault!("compile StaV code");
    };
    let Ok(mut output_file) = File::create(filename.with_extension("html")) else {
        fault!("create HTML file");
    };
    let Ok(_) = output_file.write_all(html.as_bytes()) else {
        fault!("write out to the file");
    };
}

fn stav(source: &str) -> Option<String> {
    let tokens = tokenize(source)?
        .iter()
        .map(|x| Node::parse(x.trim()))
        .collect::<Option<Vec<Node>>>()?;
    let mut stack: Stack = Vec::new();
    for token in tokens {
        token.eval(&mut stack)?;
    }
    generate(stack)
}

type Stack = Vec<Value>;

fn generate(stack: Stack) -> Option<String> {
    let mut output = Vec::new();
    for value in stack {
        let Value::Text(text) = value else {
            return None;
        };

        macro_rules! set_font_size {
            ($font_size: expr) => {
                if let Some(font_size) = $font_size {
                    format!(" style=\"font-size: {font_size}px;\"")
                } else {
                    String::new()
                }
            };
        }

        let html = match (text.tag, text.font_size) {
            (HTMLTag::Paragraph, font_size) => {
                format!("<p{}>{}</p>", set_font_size!(font_size), text.content)
            }
            (HTMLTag::Heading(level), font_size) => {
                format!(
                    "<h{level}{}>{}</h{level}>",
                    set_font_size!(font_size),
                    text.content,
                )
            }
            (HTMLTag::Link(url), font_size) => format!(
                "<a href=\"{}\"{}>{}</a>",
                url,
                set_font_size!(font_size),
                text.content,
            ),
            (HTMLTag::BlockQuote, font_size) => {
                format!(
                    "<blockquote{}>{}</blockquote>",
                    set_font_size!(font_size),
                    text.content
                )
            }
        };
        output.push(html);
    }
    Some(output.join("\n"))
}

fn tokenize(source: &str) -> Option<Vec<String>> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current_token = String::new();
    let mut in_quote = false;
    let mut is_escape = false;

    for c in source.chars() {
        if is_escape {
            current_token.push(match c {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                _ => c,
            });
            is_escape = false;
        } else {
            match c {
                '"' | '\'' | '`' => {
                    in_quote = !in_quote;
                    current_token.push(c);
                }
                '\\' if in_quote => {
                    current_token.push(c);
                    is_escape = true;
                }
                ' ' | '\n' | '\t' | '\r' if !in_quote && !current_token.is_empty() => {
                    tokens.push(current_token.clone());
                    current_token.clear();
                }
                _ => current_token.push(c),
            }
        }
    }

    if is_escape || in_quote {
        return None;
    }
    if !current_token.is_empty() {
        tokens.push(current_token.clone());
    }
    Some(tokens)
}

#[derive(Clone, Debug)]
enum Value {
    Text(Text),
    Integer(i32),
    Link(String),
}

impl Value {
    fn parse(source: &str) -> Option<Value> {
        if let Some(text) = source.strip_prefix("\"").and_then(|x| x.strip_suffix("\"")) {
            Some(Value::Text(Text {
                content: text.to_string(),
                font_size: None,
                tag: HTMLTag::Paragraph,
            }))
        } else if let Some(number) = source.parse::<i32>().ok() {
            Some(Value::Integer(number))
        } else if source.starts_with("https://") {
            Some(Value::Link(source.to_string()))
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
struct Text {
    content: String,
    font_size: Option<i32>,
    tag: HTMLTag,
}

#[derive(Clone, Debug)]
enum HTMLTag {
    Heading(i32),
    Paragraph,
    Link(String),
    BlockQuote,
}

#[derive(Clone, Debug)]
enum Node {
    Literal(Value),
    Command(Command),
}

impl Node {
    fn eval(&self, stack: &mut Stack) -> Option<()> {
        match self {
            Node::Literal(value) => stack.push(value.clone()),
            Node::Command(command) => command.eval(stack)?,
        }
        Some(())
    }

    fn parse(source: &str) -> Option<Node> {
        if let Some(value) = Value::parse(source) {
            Some(Node::Literal(value))
        } else if let Some(value) = Command::parse(source) {
            Some(Node::Command(value))
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
enum Command {
    Heading,
    FontSize,
    Link,
    BlockQuote,
    Swap,
    Pop,
}

impl Command {
    fn eval(&self, stack: &mut Stack) -> Option<()> {
        match self {
            Command::Heading => {
                let Value::Integer(level) = stack.pop()? else {
                    return None;
                };
                let Value::Text(mut text) = stack.pop()? else {
                    return None;
                };
                text.tag = HTMLTag::Heading(level);
                stack.push(Value::Text(text));
            }
            Command::FontSize => {
                let Value::Integer(size) = stack.pop()? else {
                    return None;
                };
                let Value::Text(mut text) = stack.pop()? else {
                    return None;
                };
                text.font_size = Some(size);
                stack.push(Value::Text(text));
            }
            Command::Link => {
                let Value::Link(url) = stack.pop()? else {
                    return None;
                };
                let Value::Text(mut text) = stack.pop()? else {
                    return None;
                };
                text.tag = HTMLTag::Link(url);
                stack.push(Value::Text(text));
            }
            Command::BlockQuote => {
                let Value::Text(mut text) = stack.pop()? else {
                    return None;
                };
                text.tag = HTMLTag::BlockQuote;
                stack.push(Value::Text(text));
            }
            Command::Swap => {
                let value1 = stack.pop()?;
                let value2 = stack.pop()?;
                stack.push(value1);
                stack.push(value2);
            }
            Command::Pop => {
                stack.pop()?;
            }
        }
        Some(())
    }

    fn parse(source: &str) -> Option<Command> {
        match source {
            "heading" => Some(Command::Heading),
            "font-size" => Some(Command::FontSize),
            "link" => Some(Command::Link),
            "block-quote" => Some(Command::BlockQuote),
            "swap" => Some(Command::Swap),
            "pop" => Some(Command::Pop),
            _ => None,
        }
    }
}

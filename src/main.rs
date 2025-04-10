use clap::Parser;
use std::{
    collections::HashMap,
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
        .filter(|x| !x.trim().is_empty())
        .map(|x| Node::parse(x.trim()))
        .collect::<Option<Vec<Node>>>()?;
    let mut stack: Stack = Stack {
        data: Vec::new(),
        scope: HashMap::new(),
        title: None,
        theme: None,
    };
    for token in tokens {
        token.eval(&mut stack)?;
    }
    generate(stack)
}

struct Stack {
    data: Vec<Value>,
    scope: HashMap<String, Value>,
    title: Option<String>,
    theme: Option<String>,
}

fn generate(stack: Stack) -> Option<String> {
    let mut output = Vec::new();
    let mut list = Vec::new();
    let mut is_list = false;
    for value in stack.data {
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
            (HTMLTag::Image(url), _) => {
                format!("<img src=\"{}\" alt=\"{}\">", url, text.content)
            }
            (HTMLTag::List, font_size) => {
                list.push(format!(
                    "<li {}>{}</li>",
                    set_font_size!(font_size),
                    text.content
                ));
                is_list = true;
                continue;
            }
        };
        if is_list {
            output.push(format!("<ul>{}</ul>", list.join("\n")));
            is_list = false;
            list.clear();
        }
        output.push(html);
    }
    Some(format!(
        r#"
        <html>
            <head>
                <meta charset="UTF-8">
                <title>{}</title>
                <link rel="stylesheet" href="theme/{}.css">
            </head>
            <body>
                {}
            </body>
        </html>
        "#,
        stack.title.unwrap_or("Untitled".to_owned()),
        stack.theme.unwrap_or("none".to_owned()),
        output.join("\n"),
    ))
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
                '"' => {
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

fn text_escape(text: &str) -> String {
    let mut result = String::new();
    let mut is_escape = false;
    for c in text.chars() {
        if is_escape {
            result.push(c);
            is_escape = false;
        } else {
            match c {
                '\\' => {
                    is_escape = true;
                }
                _ => result.push(c),
            }
        }
    }
    result
}

#[derive(Clone, Debug)]
enum Value {
    Text(Text),
    Integer(i32),
    Link(String),
    Symbol(String),
}

impl Value {
    fn eval(&self, stack: &mut Stack) -> Option<()> {
        match self {
            Value::Text(text) => stack.data.push(Value::Text(Text {
                content: tokenize(&text.content)?
                    .iter()
                    .map(|token| {
                        if let Some(name) = token.strip_prefix("@") {
                            stack.scope.get(name).map(|x| x.to_string())
                        } else {
                            Some(token.clone())
                        }
                    })
                    .collect::<Option<Vec<String>>>()?
                    .join(" "),
                ..text.clone()
            })),
            _ => stack.data.push(self.clone()),
        }
        Some(())
    }

    fn to_string(&self) -> String {
        match self {
            Value::Integer(int) => int.to_string(),
            Value::Text(text) => text.content.clone(),
            Value::Link(text) | Value::Symbol(text) => text.clone(),
        }
    }

    fn parse(source: &str) -> Option<Value> {
        if let Some(text) = source.strip_prefix("\"").and_then(|x| x.strip_suffix("\"")) {
            Some(Value::Text(Text {
                content: text_escape(text.replace("\\\n", "<br>").trim()),
                font_size: None,
                tag: HTMLTag::Paragraph,
            }))
        } else if let Some(number) = source.parse::<i32>().ok() {
            Some(Value::Integer(number))
        } else if source.starts_with("https://") {
            Some(Value::Link(source.to_string()))
        } else if let Some(name) = source.strip_prefix("@") {
            Some(Value::Symbol(name.to_string()))
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
    Image(String),
    BlockQuote,
    List,
}

#[derive(Clone, Debug)]
enum Node {
    Literal(Value),
    Command(Command),
}

impl Node {
    fn eval(&self, stack: &mut Stack) -> Option<()> {
        match self {
            Node::Literal(value) => value.eval(stack)?,
            Node::Command(command) => command.eval(stack)?,
        }
        Some(())
    }

    fn parse(source: &str) -> Option<Node> {
        if let Some(value) = Command::parse(source) {
            Some(Node::Command(value))
        } else if let Some(value) = Value::parse(source) {
            Some(Node::Literal(value))
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
    Image,
    List,
    Title,
    Theme,
    Load,
    Store,
    Concat,
    Dup,
    Swap,
    Pop,
}

impl Command {
    fn eval(&self, stack: &mut Stack) -> Option<()> {
        match self {
            Command::Heading => {
                let Value::Integer(level) = stack.data.pop()? else {
                    return None;
                };
                let Value::Text(mut text) = stack.data.pop()? else {
                    return None;
                };
                text.tag = HTMLTag::Heading(level);
                stack.data.push(Value::Text(text));
            }
            Command::FontSize => {
                let Value::Integer(size) = stack.data.pop()? else {
                    return None;
                };
                let Value::Text(mut text) = stack.data.pop()? else {
                    return None;
                };
                text.font_size = Some(size);
                stack.data.push(Value::Text(text));
            }
            Command::Link => {
                let Value::Link(url) = stack.data.pop()? else {
                    return None;
                };
                let Value::Text(mut text) = stack.data.pop()? else {
                    return None;
                };
                text.tag = HTMLTag::Link(url);
                stack.data.push(Value::Text(text));
            }
            Command::BlockQuote => {
                let Value::Text(mut text) = stack.data.pop()? else {
                    return None;
                };
                text.tag = HTMLTag::BlockQuote;
                stack.data.push(Value::Text(text));
            }
            Command::Image => {
                let Value::Link(url) = stack.data.pop()? else {
                    return None;
                };
                let text = Text {
                    content: String::new(),
                    font_size: None,
                    tag: HTMLTag::Image(url),
                };
                stack.data.push(Value::Text(text));
            }
            Command::List => {
                let Value::Text(mut text) = stack.data.pop()? else {
                    return None;
                };
                text.tag = HTMLTag::List;
                stack.data.push(Value::Text(text));
            }
            Command::Title => {
                let Value::Text(text) = stack.data.pop()? else {
                    return None;
                };
                stack.title = Some(text.content);
            }
            Command::Theme => {
                let Value::Text(text) = stack.data.pop()? else {
                    return None;
                };
                stack.theme = Some(text.content);
            }
            Command::Load => {
                let Value::Symbol(name) = stack.data.pop()? else {
                    return None;
                };
                stack.data.push(stack.scope.get(&name)?.clone())
            }
            Command::Store => {
                let Value::Symbol(name) = stack.data.pop()? else {
                    return None;
                };
                let value = stack.data.pop()?;
                stack.scope.insert(name, value);
            }
            Command::Concat => {
                let Value::Text(text2) = stack.data.pop()? else {
                    return None;
                };
                let Value::Text(mut text1) = stack.data.pop()? else {
                    return None;
                };
                text1.content.push_str(&text2.content);
                stack.data.push(Value::Text(text1));
            }
            Command::Dup => {
                let value = stack.data.pop()?;
                stack.data.push(value.clone());
                stack.data.push(value);
            }
            Command::Swap => {
                let value1 = stack.data.pop()?;
                let value2 = stack.data.pop()?;
                stack.data.push(value1);
                stack.data.push(value2);
            }
            Command::Pop => {
                stack.data.pop()?;
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
            "list" => Some(Command::List),
            "image" => Some(Command::Image),
            "title" => Some(Command::Title),
            "theme" => Some(Command::Theme),
            "load" => Some(Command::Load),
            "concat" => Some(Command::Concat),
            "store" => Some(Command::Store),
            "dup" => Some(Command::Dup),
            "swap" => Some(Command::Swap),
            "pop" => Some(Command::Pop),
            _ => None,
        }
    }
}

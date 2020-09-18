#[derive(Clone)]
pub struct Args {
    pub height: u32,
    pub buttons: Vec<ArgButton>,
    pub font: String,
}

#[derive(Clone)]
pub struct ArgButton {
    pub text: String,
    pub action: String,
}

pub fn parse(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut height: u32 = 32;
    let mut buttons = vec![];
    let mut font = String::from("/usr/share/fonts/TTF/Symbola.ttf");

    // skip the binary name
    let mut args = args.skip(1);

    loop {
        match args.next().as_deref() {
            // parse bar height
            Some("-h") | Some("--height") => {
                let arg = args.next();

                if arg.is_some() {
                    match arg.unwrap().parse::<u32>() {
                        Ok(value) => {
                            height = value;
                        }
                        Err(err) => return Err(format!("invalid height '{}'", err)),
                    }
                } else {
                    return Err("missing required arg message (-h/--height)".into());
                }
            }

            // parse font path
            Some("-f") | Some("--font") => {
                let arg = args.next();

                if arg.is_some() {
                    font = arg.unwrap();
                }
            }

            // parse button command
            Some("-b") | Some("--button") | Some("-B") | Some("--button-no-terminal") => {
                let text = args.next();
                let action = args.next();

                match (text, action) {
                    (Some(text), Some(action)) => buttons.push(ArgButton { text, action }),
                    (None, _) => return Err("button missing text".into()),
                    (Some(_), None) => return Err("button missing action".into()),
                }
            }
            Some(arg) => return Err(format!("invalid arg '{}'", arg)),
            None => break,
        }
    }

    if buttons.len() > 0 {
        Ok(Args {
            height,
            buttons,
            font,
        })
    } else {
        Err("bad parameters".into())
    }
}

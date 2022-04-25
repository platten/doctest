// SPDX-FileCopyrightText: 2022 Profian Inc. <opensource@profian.com>
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};

use anyhow::Result;
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};
use regex::Regex;

trait CodeBlockKindExt {
    /// Determines whether this code block should be included in output.
    ///
    /// This is based on matching KEY=VALUE filters from `/etc/os-release`.
    fn include(&self, os: &HashMap<String, String>) -> bool;
}

impl CodeBlockKindExt for CodeBlockKind<'_> {
    fn include(&self, os: &HashMap<String, String>) -> bool {
        match self {
            Self::Fenced(kind) => {
                let mut split = kind.as_ref().split(':');

                match split.next() {
                    Some("sh") => {
                        for filter in split {
                            match filter.split_once('=') {
                                Some((k, v)) if os.get(k).map(|x| &**x) == Some(v) => continue,
                                _ => return false,
                            }
                        }

                        true
                    }

                    _ => false,
                }
            }

            _ => false,
        }
    }
}

/// Returns an iterator over the command lines in code blocks based on OS filters.
fn filter_markdown(os: impl Read, md: &str) -> Result<impl '_ + Iterator<Item = String>> {
    // Read the distribution variables.
    let mut distro = HashMap::new();
    for line in BufReader::new(os).lines() {
        if let Some((lhs, rhs)) = line?.split_once('=') {
            distro.insert(lhs.to_owned(), rhs.to_owned());
        }
    }

    // Filter the command blocks using the os filters.
    let mut dump = false;
    Ok(Parser::new(md).filter_map(move |event| match event {
        Event::Start(Tag::CodeBlock(block)) if block.include(&distro) => {
            dump = true;
            None
        }

        Event::End(Tag::CodeBlock(CodeBlockKind::Fenced(..))) => {
            dump = false;
            None
        }

        Event::Text(text) if dump => Some(text.to_string()),
        _ => None,
    }))
}

fn main() -> Result<()> {
    let mut args = std::env::args();

    let cmd = args.next().unwrap();

    let md = match args.next() {
        Some(md) => std::fs::read_to_string(md)?,
        None => {
            eprintln!("Usage: {} <markdown> <os-release>", cmd);
            std::process::exit(1);
        }
    };

    let re = Regex::new(r"^\s*[\$|#]\s*(?P<command>.+?)\s*").unwrap();
    let os = args.next().unwrap_or_else(|| "/etc/os-release".to_owned());

    for cmd in filter_markdown(File::open(os)?, &md)? {
        for line in cmd.lines() {
            let cleaned_line = re.replace_all(line, "$command").to_string();
            if cleaned_line.len() > 1 {
                println!("{}", cleaned_line);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use crate::filter_markdown;

    const OS_RELEASE: &str = r#"
PRETTY_NAME="Debian GNU/Linux 11 (bullseye)"
NAME="Debian GNU/Linux"
VERSION_ID="11"
VERSION="11 (bullseye)"
VERSION_CODENAME=bullseye
ID=debian
HOME_URL="https://www.debian.org/"
SUPPORT_URL="https://www.debian.org/support"
BUG_REPORT_URL="https://bugs.debian.org/"
"#;

    const MARKDOWN: &str = r#"
# Welcome!

Welcome to Enarx.

# Getting Started

## Install Dependencies
### Fedora

```sh:ID=fedora
echo fedora
```

### Debian

```sh:ID=debian
echo debian
```

## Build Enarx

```sh
echo enarx
```
"#;

    #[test]
    fn test() {
        let mut os = OS_RELEASE.as_bytes();

        assert_eq!(
            filter_markdown(&mut os, MARKDOWN)
                .unwrap()
                .collect::<String>(),
            "echo debian\necho enarx\n"
        );
    }
}

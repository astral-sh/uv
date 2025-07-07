use crate::shell_out;

pub fn zsh() -> Result<(), anyhow::Error> {
    let shell = std::env::var("SHELL");
    if shell.is_err() || !shell.as_ref().unwrap().ends_with("zsh") {
        return Err(anyhow::anyhow!(
            "This script is intended for Zsh shell only."
        ));
    }
    shell_out!(
        r#"function envy() {{
    while IFS= read -r line; do
        if [[ "$line" == \<ENVY\>\ * ]]; then
            cmd="${{line#<ENVY> }}"
            eval "$cmd"
        else
            echo "$line"
        fi
    done <<< "$(/home/kshah/Documents/Github/uv/target/debug/uv envy "$@")"
}}
"#
    );
    Ok(())
}

pub fn bash() -> Result<(), anyhow::Error> {
    let shell = std::env::var("SHELL");
    if shell.is_err() || !shell.as_ref().unwrap().ends_with("bash") {
        return Err(anyhow::anyhow!(
            "This script is intended for Bash shell only."
        ));
    }
    todo!();
}

pub fn fish() -> Result<(), anyhow::Error> {
    let shell = std::env::var("SHELL");
    if shell.is_err() || !shell.as_ref().unwrap().ends_with("fish") {
        return Err(anyhow::anyhow!(
            "This script is intended for Fish shell only."
        ));
    }
    todo!();
}

pub fn powershell() -> Result<(), anyhow::Error> {
    todo!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zsh_ok() {
        unsafe {
            std::env::set_var("SHELL", "/bin/zsh");
        }
        assert!(zsh().is_ok());
    }

    #[test]
    fn test_zsh_fail() {
        unsafe {
            std::env::set_var("SHELL", "/bin/bash");
        }
        assert!(zsh().is_err());
    }

    #[test]
    #[should_panic(expected = "not yet implemented")]
    fn test_bash_ok() {
        unsafe {
            std::env::set_var("SHELL", "/bin/bash");
        }
        bash();
    }

    #[test]
    fn test_bash_fail() {
        unsafe {
            std::env::set_var("SHELL", "/bin/zsh");
        }
        assert!(bash().is_err());
    }

    #[test]
    #[should_panic(expected = "not yet implemented")]
    fn test_fish_ok() {
        unsafe {
            std::env::set_var("SHELL", "/bin/fish");
        }
        fish();
    }

    #[test]
    fn test_fish_fail() {
        unsafe {
            std::env::set_var("SHELL", "/bin/bash");
        }
        assert!(fish().is_err());
    }

    #[test]
    #[should_panic(expected = "not yet implemented")]
    fn test_powershell() {
        // Powershell is not implemented yet, so we just check that it compiles.
        powershell();
    }
}

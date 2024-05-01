pub mod groq;
pub mod openai;

pub async fn get_api_key(api_key_command: &[String]) -> anyhow::Result<Option<String>> {
    if api_key_command.is_empty() {
        return Ok(None);
    }
    let bin = api_key_command.first().unwrap();
    let args = api_key_command.iter().skip(1);
    let mut cmd = std::process::Command::new(bin);
    cmd.args(args);
    let output = cmd.output()?;
    let api_key = String::from_utf8(output.stdout)?.trim().to_string();

    Ok(Some(api_key))
}

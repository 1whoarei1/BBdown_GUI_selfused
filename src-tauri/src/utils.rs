use encoding_rs::GBK;

pub fn hide_console_window(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
    }
}

pub fn decode_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut data = Vec::with_capacity(stdout.len() + stderr.len());
    data.extend_from_slice(stdout);
    data.extend_from_slice(stderr);

    match String::from_utf8(data.clone()) {
        Ok(text) => text,
        Err(_) => {
            let (text, _, _) = GBK.decode(&data);
            text.into_owned()
        }
    }
}

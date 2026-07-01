use std::process::Stdio;

use tracing::{debug, trace};

#[tracing::instrument]
pub async fn update_yt_dlp() -> anyhow::Result<()> {
    debug!("Checking for yt-dlp updates");
    let mut cmd = {
        let mut cmd = tokio::process::Command::new("yt-dlp");
        cmd.arg("--ignore-config");
        cmd.arg("--update");
        cmd.stdin(Stdio::null());

        cmd
    };

    trace!(?cmd, "Updating yt-dlp");

    let res = cmd.output().await?;

    trace!(?res, "yt-dlp update result");

    if !res.status.success() {
        anyhow::bail!(
            "yt-dlp update failed: {output}",
            output = String::from_utf8_lossy(&res.stderr)
        );
    }

    Ok(())
}

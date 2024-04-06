use crate::utils;
use std::path::{Path, PathBuf};
use url::Url;

pub struct ArtifactoryComm;

impl ArtifactoryComm {
    pub(crate) fn Download(
        url: &Url,
        base_path: PathBuf,
        token: Option<String>,
    ) -> Result<PathBuf, String> {
        let last_part = url.path_segments().unwrap().into_iter().last().unwrap();
        let path = PathBuf::from(last_part);
        let path = base_path.join(path);

        let curl_command = format!(
            r#"curl -L -O {} -o {}"#,
            url,
            path.to_str().unwrap()
        );

        println!("{}", curl_command);
        let bearer = format!("Authorization: Bearer {}", token.unwrap());
        utils::bash_exec::BashExec::exec_wait(&bearer, &url.to_string(), path.to_str().unwrap());

        Ok(path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[test]
    fn simple_test() {
        let token ="";
        let url = "https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK.Cpp/2.4.0/amd64/sdk-cpp-demo_2.4.0_amd64.snap";
        let url = Url::parse(url).unwrap();
        let token = Some(String::from(token));
        let path = "/home/phantom-il-alex/jfrog";
        let base_path = Path::new(path);
        ArtifactoryComm::Download(&url, base_path.to_path_buf(), token);
    }
}

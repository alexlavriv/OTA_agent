use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

pub struct ProgressUI;

impl ProgressUI{
    pub fn init(){
        let index = "index.html";
        let style = "styles.css";
        let src = "script.js";
        let favicon = "phantom_logo.svg";

        let index_html = include_str!("../ui/index.html");
        let styles_css = include_str!("../ui/styles.css");
        let src_js = include_str!("../ui/script.js");
        let favicon_svg = include_bytes!("../ui/phantom_logo.svg");
        ProgressUI::create_file(index, index_html);
        ProgressUI::create_file(style, styles_css);
        ProgressUI::create_file(src, src_js);
        ProgressUI::create_bytes_file(favicon, favicon_svg);
    }
    pub fn show(){
        let index_path = Path::new(".\\index.html");
        if index_path.exists(){
            open::that(".\\index.html").expect("error");
         }
         else{
            log::error!("Path {} does not exist, probably ProgressUI::show called without init",index_path.to_string_lossy());
         }
    
    }
    
    pub fn create_file(file_name: &str, content: &str){
        let mut file = File::create(file_name).expect("error");
        file.write_all(content.as_bytes()).expect("errror");
    
    }
    fn create_bytes_file(file_name: &str, content: &[u8]){
        let mut file = File::create(file_name).expect("error");
        file.write_all(content).expect("errror");
    
    }
}
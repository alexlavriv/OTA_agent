use crate::ui::progress_ui::ProgressUI;
use crate::utils::bash_exec::BashExec;
pub struct ResourceManager;

impl ResourceManager{
    const SCHEDULED_TASK_FILE_NAME: &'static str = "scheduled_task.xml";
    pub fn init(){

        let scheduled_task_xml = include_str!("scheduled_task.xml");
       
        ProgressUI::create_file(ResourceManager::SCHEDULED_TASK_FILE_NAME, scheduled_task_xml);
        ResourceManager::import_scheduled_task_inner(BashExec::exec_arg)
    }

    fn import_scheduled_task_inner(exec: fn(&str, args: &[&str]) -> Result<String, String>){
        let scheduled_task_name = "StartPhantomAgent";
        let depricated_scheduled_task_name = "Start Phantom Agent";

        let exec_result = (exec)("schtasks", &["/delete", "/tn", depricated_scheduled_task_name, "/f"]);
        match exec_result {
            Ok(message) => log::info!("Delete Agent task result OK with message {}", message),
            Err(message) => log::error!("Delete Agent task result error with message {}", message),
        }
        //schtasks /delete /tn StartPhantomAgent /f
        let exec_result = (exec)("schtasks", &["/delete", "/tn", scheduled_task_name, "/f"]);
        match exec_result {
            Ok(message) => log::info!("Delete Agent task result OK with message {}", message),
            Err(message) => log::error!("Delete Agent task result error with message {}", message),
        }
        //schtasks /Create /XML scheduled_task.xml /tn StartPhantomAgent
        let file_name = "scheduled_task.xml";
        let exec_result = (exec)("schtasks", &["/Create", "/XML", file_name, "/tn", scheduled_task_name]);
        match exec_result {
            Ok(message) => log::info!("Create Agent task result OK with message {}", message),
            Err(message) => log::error!("Create Agent task result error with message {}", message),
        }
    }
}

#[test]
fn test_import_scheduled_task_inner(){
    println!("test import_scheduled_task_inner");
    let exec_mock = |exec_name: &str, args:&[&str]| -> Result<String, String>{
        println!("exec_name {} {:?}", exec_name, args );
        let expected_first_call = r#"schtasks /delete /tn Start Phantom Agent /f"#;
        let expected_second_call = r#"schtasks /delete /tn StartPhantomAgent /f"#;
        let expected_third_call = r#"schtasks /Create /XML scheduled_task.xml /tn StartPhantomAgent"#;
        let mut vec_args : Vec::<String> =vec!(exec_name.to_string());
        for arg in args {
            vec_args.push(arg.to_string());
        }
        let actual = vec_args.join(" ");
        println!("expected_first_call: \t\t{}", expected_first_call);
        println!("expected_second_call: \t\t{}", expected_second_call);
        println!("expected_third_call: \t\t{}", expected_third_call);
        println!("actual: \t\t\t{}", actual);
        assert!(expected_first_call == actual || expected_second_call == actual || expected_third_call == actual);
        Ok(String::from(""))
    };
    ResourceManager::import_scheduled_task_inner(exec_mock);
}



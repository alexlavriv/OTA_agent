use regex::Regex;

#[allow(dead_code)]
struct ArpScan {
    pub host_file: String,
    cmd_exec: fn(commnand: &str) -> Result<String, String>, //pub entries :Vec<ArpEntry>
}
#[allow(dead_code)]
struct ArpEntry {
    ip: String,
    mac: String,
    description: String,
}

#[allow(dead_code)]
impl ArpScan {
    pub fn new(cmd: fn(commnand: &str) -> Result<String, String>, host_file: &str) -> ArpScan {
        ArpScan {
            host_file: String::from(host_file),
            cmd_exec: cmd,
        }
    }
    pub fn scan(self) -> Vec<ArpEntry> {
        vec![ArpEntry {
            ip: String::from("p"),
            mac: String::from("p"),
            description: String::from("p"),
        }]
    }
    fn parse(arps: &str) -> Vec<ArpEntry> {
        arps.lines().map(ArpScan::parse_entry).collect()
    }
    fn parse_entry(arps: &str) -> ArpEntry {
        let regex_pattern = "((?:\\d+\\.){3}\\d+)\\s+((?:[0-9|a-f]{2}:){5}[0-9|a-f]{2})\\s+(.*)";
        let re = Regex::new(regex_pattern).unwrap();

        let res = re.captures(arps).unwrap();

        ArpEntry {
            ip: String::from(res.get(1).map_or("", |m| m.as_str())),
            mac: String::from(res.get(2).map_or("", |m| m.as_str())),
            description: String::from(res.get(3).map_or("", |m| m.as_str())),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn simple_test() {
        let cmd_func = |_x: &str| -> Result<String, String> {
            let result = String::from("10.0.0.202      00:30:6f:40:18:fb       SEYEON TECH. CO., LTD. \n12.0.0.200      e0:62:90:31:da:11       Jinan Jovision Science & Technology Co., Ltd.");
            Ok(result)
        };
        let scanner = ArpScan::new(cmd_func, "file");
        let arps = scanner.scan();
        assert_eq!(arps.len(), 1);
    }
    #[test]
    fn parse_test() {
        let result = String::from("10.0.0.202      00:30:6f:40:18:fb       SEYEON TECH. CO., LTD. \n12.0.0.200      e0:62:90:31:da:11       Jinan Jovision Science & Technology Co., Ltd.");

        let scanner = ArpScan::parse(&result);
        assert_eq!(scanner.len(), 2);
    }
    #[test]
    fn parse_entry_test() {
        let result = String::from("10.0.0.202      00:30:6f:40:18:fb       SEYEON TECH. CO., LTD.");

        let parsed = ArpScan::parse_entry(&result);
        assert!(!parsed.ip.is_empty());
        assert_eq!(parsed.ip, "10.0.0.202");
        assert_eq!(parsed.mac, "00:30:6f:40:18:fb");
        assert_eq!(parsed.description, "SEYEON TECH. CO., LTD.");
    }
}

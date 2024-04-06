use ipnet::IpBitAnd;
use std::net::Ipv4Addr;

use log;
use regex::Regex;

type NetworkTestFunction = Box<dyn Fn(String, &Ipv4Addr, &Ipv4Addr) -> Option<f32>>;

// For non-linux - just ping
pub fn get_network_test(exec: fn(&str) -> Result<String, String>) -> NetworkTestFunction {
    let parser = |ping_result: String| {
        // If the ping was succesfull we will extract the time=
        return if ping_result.contains("TTL=") {
            let regex_pattern = "time<?=?(\\d+\\.?\\d*)";

            let re = Regex::new(regex_pattern).unwrap();
            let res = re.captures(&ping_result).unwrap();

            Some(res.get(1).unwrap().as_str().parse::<f32>().unwrap())
        } else {
            None
        };
    };

    Box::new(move |_interface, _ip, _getaway| {
        // Time out after 1 second, echo false on timeout
        let command = "ping 8.8.8.8 -n 1 -w 1000".to_string();
        let ping_res = exec(&command);

        match ping_res {
            Ok(res) => parser(res),
            Err(err) => {
                log::error!("Error occurred during running ping command {}", err);
                None
            }
        }
    })
}

// For linux, will try to set as default route, if not
pub fn get_interface_test(exec: fn(&str) -> Result<String, String>) -> NetworkTestFunction {
    let parser = |ping_result: String| {
        return if ping_result.contains("icmp_seq") {
            let regex_pattern = "time=(\\d+\\.?\\d*)";
            let re = Regex::new(regex_pattern).unwrap();
            let res = re.captures(&ping_result).unwrap();

            Some(res.get(1).unwrap().as_str().parse::<f32>().unwrap())
        } else {
            None
        };
    };
    // Return value
    Box::new(move |interface, _ip, gateway| {
        // Time out after 1 second, echo false on timeout
        let command = format!("timeout 1 ping -I {interface} -c 1 8.8.8.8");

        // If the route is not default, we will set it default and remove afterward.
        // Otherwise the ping command doesn't return result, even if the network is available
        let mut set_default = false;
        if !is_default_route(&interface, exec) {
            set_default_route(gateway, exec);
            set_default = true;
        }
        let ping_res = exec(&command);

        if set_default {
            delete_default_route(gateway, &interface, exec);
        }
        match ping_res {
            Ok(res) => parser(res),
            Err(err) => {
                log::error!("Error occurred during running ping command {}", err);
                None
            }
        }
    })
}

// This function gest ip address and netmask, and caclculates the minimum gateway
pub fn get_gateway(ip: &Ipv4Addr, netmask: &Ipv4Addr) -> Ipv4Addr {
    let subnet = ip.bitand(*netmask);
    let octets = subnet.octets();
    Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3] + 1)
}

fn run_command(command: &str, exec: fn(&str) -> Result<String, String>) -> bool {
    let res = exec(command);
    res.is_ok()
}
fn is_default_route(interface: &str, exec: fn(&str) -> Result<String, String>) -> bool {
    let command = "route -n";
    let res = exec(command);
    let parser = |output: String| {
        for line in output.lines() {
            if line.contains(interface) && line.contains("UG") {
                return true;
            }
        }
        false
    };
    match res {
        Ok(output) => parser(output),
        Err(_) => false,
    }
}
fn set_default_route(gateway: &Ipv4Addr, exec: fn(&str) -> Result<String, String>) -> bool {
    // sudo ip route add default via 192.168.201.1
    let command = format!("sudo ip route add default via {gateway}");
    run_command(&command, exec)
}

fn delete_default_route(
    gateway: &Ipv4Addr,
    interface: &str,
    exec: fn(&str) -> Result<String, String>,
) -> bool {
    // sudo ip route del 0.0.0.0 via 192.168.202.1
    // sudo route delete default gw 192.168.202.1  eno1.20
    let command = format!("sudo route delete default gw {gateway} {interface}");
    run_command(&command, exec)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn simple_test() {
        let f = get_interface_test(|_| {
            Ok("64 bytes from 8.8.8.8: icmp_seq=1 ttl=115 time=55.6 ms".to_string())
        });
        let f2 = get_interface_test(|_| {
            Ok(r#"
                PING 8.8.8.8 (8.8.8.8) from 192.168.201.20 eno1.10: 56(84) bytes of data.
64 bytes from 8.8.8.8: icmp_seq=1 ttl=109 time=205 ms
                "#
            .to_string())
        });
        let ip = Ipv4Addr::new(0, 0, 0, 0);
        assert_eq!(f(String::from("alex"), &ip, &ip), Some(55.6));
        let f = get_interface_test(|_| Ok("false".to_string()));
        assert_eq!(f(String::from("alex"), &ip, &ip), None);

        assert_eq!(f2(String::from("alex"), &ip, &ip), Some(205.0));
    }
    #[test]
    fn gateway_test() {
        let ip = Ipv4Addr::new(192, 168, 100, 50);
        let mask = Ipv4Addr::new(255, 255, 255, 0);
        let gate_way = Ipv4Addr::new(192, 168, 100, 1);
        let gate = get_gateway(&ip, &mask);
        println!("{}", gate);
        assert_eq!(get_gateway(&ip, &mask), gate_way);

        let ip = Ipv4Addr::new(192, 168, 100, 50);
        let mask = Ipv4Addr::new(255, 255, 255, 254);
        let gate_way = Ipv4Addr::new(192, 168, 100, 51);
        let gate = get_gateway(&ip, &mask);
        println!("{}", gate);
        assert_eq!(get_gateway(&ip, &mask), gate_way);
    }
    #[test]
    fn is_default_gateway_test() {
        let bash_mock = |_: &str| -> Result<String, String> {
            let res = r#"
            Kernel IP routing table
Destination     Gateway         Genmask         Flags Metric Ref    Use Iface
0.0.0.0         192.168.43.121  0.0.0.0         UG    20600  0        0 wlp0s20f3
10.231.245.0    0.0.0.0         255.255.255.0   U     0      0        0 mpqemubr0
169.254.0.0     0.0.0.0         255.255.0.0     U     1000   0        0 mpqemubr0
172.17.0.0      0.0.0.0         255.255.0.0     U     0      0        0 docker0
172.18.0.0      0.0.0.0         255.255.0.0     U     0      0        0 br-4b9f6c26356e
192.30.50.0     0.0.0.0         255.255.255.0   U     0      0        0 bridgemain
192.40.60.0     0.0.0.0         255.255.255.0   U     0      0        0 bridgenet1
192.50.60.0     0.0.0.0         255.255.255.0   U     0      0        0 bridgenet2
192.168.43.0    0.0.0.0         255.255.255.0   U     600    0        0 wlp0s20f3
            "#;
            Ok(String::from(res))
        };
        let ans = is_default_route("wlp0s20f3", bash_mock);
        assert!(ans);
        let ans = is_default_route("mpqemubr0", bash_mock);
        assert!(!ans);
    }

    #[test]
    fn windows_network_test() {
        let bash_mock = |_: &str| -> Result<String, String> {
            let res = r#"
            Pinging 8.8.8.8 with 32 bytes of data:
            Reply from 8.8.8.8: bytes=32 time=64ms TTL=56
            
            Ping statistics for 8.8.8.8:
                Packets: Sent = 1, Received = 1, Lost = 0 (0% loss),
            Approximate round trip times in milli-seconds:
                Minimum = 64ms, Maximum = 64ms, Average = 64ms
            "#;
            Ok(String::from(res))
        };

        let tester = get_network_test(bash_mock);
        let ip = Ipv4Addr::new(0, 0, 0, 0);
        let result = tester(String::from("alex"), &ip, &ip).unwrap();
        assert_eq!(result, 64.0)
    }

    #[test]
    fn windows_network_test2() {
        let bash_mock = |_: &str| -> Result<String, String> {
            let res = r#"
    Pinging 8.8.8.8 with 32 bytes of data:
Reply from 8.8.8.8: bytes=32 time<1ms TTL=111
Ping statistics for 8.8.8.8:
    Packets: Sent = 1, Received = 1, Lost = 0 (0% loss),
Approximate round trip times in milli-seconds:
    Minimum = 0ms, Maximum = 0ms, Average = 0ms
    "#;
            Ok(String::from(res))
        };

        let tester = get_network_test(bash_mock);
        let ip = Ipv4Addr::new(0, 0, 0, 0);
        let result = tester(String::from("alex"), &ip, &ip).unwrap();
        assert_eq!(result, 1.0)
    }

    #[test]
    fn windows_network_test_not_available() {
        let bash_mock = |_: &str| -> Result<String, String> {
            let res = r#"
            Pinging 8.8.8.1 with 32 bytes of data:
            Request timed out.
            
            Ping statistics for 8.8.8.1:
                Packets: Sent = 1, Received = 0, Lost = 1 (100% loss),
            "#;
            Ok(String::from(res))
        };

        let tester = get_network_test(bash_mock);
        let ip = Ipv4Addr::new(0, 0, 0, 0);
        let result = tester(String::from("alex"), &ip, &ip);
        assert_eq!(result, None)
    }
}

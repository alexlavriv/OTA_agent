function log(message) {
    console.log(message)
}

log("my test")

async function fetchAsync(url) {
    const response = await fetch(url);
    const status = await response.json();
    set_status_gui(status);
}

function getAgentStatus() {
    const url = 'http://localhost:30000/status';
    (async () => await fetchAsync(url))();

}
function value_to_text(value){
    switch(value.ota_status) {
        case 'error':
            return "Error"
        case 'installing':
            return `Installing ${value.component_name}`
        case 'downloading':
            return "Downloading updates"
        case 'checking':
          return "Checking for updates"
        case 'updated':
          return "The sofware is up to date"
        default:
          return "Unknown status"
      }
}
function set_element_value(id, value){
    let item = document.getElementById(id);
    if (value != undefined){
        item.innerHTML = value;
        
    }else{
        item.innerHTML = '';
    }
}
function clear_eta_element(){
    let item = document.getElementById("eta");
    item.innerHTML = '';
}
function format_seconds(seconds){
    return new Date(seconds * 1000).toISOString().slice(11, 19);
}
function set_status_gui(status) {
    console.log(status)

    set_element_value("manifest_version", status.manifest_version)
    set_element_value("ota_status", value_to_text(status))

    if ('eta' in status){
        set_element_value("eta", format_seconds(status.eta))
    }else{
        clear_eta_element();
    }


    if (status.ota_status === "updated") {
        ota_status.classList.remove("red")
        ota_status.classList.remove("yellow")
        ota_status.classList.add("green");
    } else if (status.ota_status === "error"){
        ota_status.classList.remove("green")
        ota_status.classList.remove("yellow")
        ota_status.classList.add("red");
    } else if (status.ota_status === "checking" || status.ota_status === "installing" || status.ota_status === "downloading"){
        ota_status.classList.remove("green")
        ota_status.classList.add("yellow")
        ota_status.classList.remove("red");
    }
        let message = document.getElementById("message");
    message.innerHTML = status.message
}

window.onload = function () { // Or window.addEventListener("load", function() {
    getAgentStatus()
    setInterval(getAgentStatus, 1000);

}

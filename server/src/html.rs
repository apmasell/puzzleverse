use horrorshow::prelude::*;

#[cfg(feature = "wasm-client")]
pub fn create_main() -> String {
  let contents = horrorshow::html! {
       : horrorshow::helper::doctype::HTML;
       head {
           title: "Puzzleverse";
           css {
              : Raw("html, body { margin: 0 !important; padding: 0 !important; } canvas { position:fixed; left:0; top:0; width:100%; height:100%; }")
           }
           script(type="module"){
              : "import init from './puzzleverse-client.js'; init();"
           }
       }
       body {
           canvas(id="puzzleverse");
       }
  };
  contents.into_string().unwrap()
}

#[cfg(not(feature = "wasm-client"))]
pub fn create_main() -> String {
  let contents = horrorshow::html! {
       : horrorshow::helper::doctype::HTML;
       head {
           title: "Puzzleverse";
       }
       body {
           p {
               : "Please download a Puzzleverse client and connect to this server.";
           }
       }
  };
  contents.into_string().unwrap()
}

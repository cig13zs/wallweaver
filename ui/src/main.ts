import "./styles/ww.css";
import "./styles/base.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { applyAppearance, loadAppearance } from "./lib/stores/theme";

applyAppearance(loadAppearance());

const target = document.getElementById("app");
if (!target) throw new Error("missing #app");
export default mount(App, { target });

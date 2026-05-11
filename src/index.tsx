/* @refresh reload */
import { render } from "solid-js/web";
import "highlight.js/styles/github-dark.css";
import App from "./App";

render(() => <App />, document.getElementById("root") as HTMLElement);

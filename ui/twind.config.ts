import { Options } from "$fresh/plugins/twind.ts";
import colors from "https://esm.sh/tailwindcss@3.2.4/src/public/colors";

export default {
  selfURL: import.meta.url,
  theme: {
    colors,
  }
} as Options;

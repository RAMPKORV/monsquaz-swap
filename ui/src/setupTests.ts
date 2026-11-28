import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";

// The demo client has deliberate delays (up to 420 ms). A loaded CI worker
// must still see the result, so the default 1 s async timeout is too tight.
configure({ asyncUtilTimeout: 3_000 });

afterEach(cleanup);

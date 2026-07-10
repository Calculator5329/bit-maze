import "./globals.css";

export const metadata = {
  title: "bit-maze // trial build",
  description: "A binary-native puzzle game powered by packed bitplanes and BitVM.",
};

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}

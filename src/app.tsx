import { Box, Text, useApp, useInput } from "ink";

export function formatPrompt(prompt: string): string {
  const normalized = prompt.trim().replace(/\s+/g, " ");
  return normalized || "Ready";
}

export function App(): React.JSX.Element {
  const { exit } = useApp();

  useInput((input, key) => {
    if (input === "q" || key.escape) {
      exit();
    }
  });

  return (
    <Box flexDirection="column" padding={1}>
      <Text color="cyan" bold>
        Talaria
      </Text>
      <Text>Hermes-native terminal UI prototype</Text>
      <Text dimColor>{formatPrompt("Press q or Esc to exit")}</Text>
    </Box>
  );
}

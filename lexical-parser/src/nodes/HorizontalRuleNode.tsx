import { JSX } from "npm:react/jsx-runtime";
import { addClassNamesToElement } from "npm:@lexical/utils";
import {
  $applyNodeReplacement,
  DecoratorNode,
  DOMConversionOutput,
  LexicalNode,
  SerializedLexicalNode,
} from "npm:lexical";
import { EditorConfig } from "npm:lexical";
import { DOMConversionMap, DOMExportOutput } from "npm:lexical";

export type SerializedHorizontalRuleNode = SerializedLexicalNode;

export class HorizontalRuleNode extends DecoratorNode<JSX.Element> {
  static override getType(): string {
    return "horizontalrule";
  }

  static override clone(node: HorizontalRuleNode): HorizontalRuleNode {
    return new HorizontalRuleNode(node.__key);
  }

  static override importJSON(
    serializedNode: SerializedHorizontalRuleNode
  ): HorizontalRuleNode {
    return $createHorizontalRuleNode().updateFromJSON(serializedNode);
  }

  static override importDOM(): DOMConversionMap | null {
    return {
      hr: () => ({
        conversion: $convertHorizontalRuleElement,
        priority: 0,
      }),
    };
  }

  override exportDOM(): DOMExportOutput {
    return { element: document.createElement("hr") };
  }

  override createDOM(config: EditorConfig): HTMLElement {
    const element = document.createElement("hr");
    addClassNamesToElement(element, config.theme.hr);
    return element;
  }

  override getTextContent(): string {
    return "\n";
  }

  override isInline(): false {
    return false;
  }

  override updateDOM(): boolean {
    return false;
  }

  override decorate(): JSX.Element {
    // deno-lint-ignore jsx-no-useless-fragment
    return <></>;
  }
}

function $convertHorizontalRuleElement(): DOMConversionOutput {
  return { node: $createHorizontalRuleNode() };
}

export function $createHorizontalRuleNode(): HorizontalRuleNode {
  return $applyNodeReplacement(new HorizontalRuleNode());
}

export function $isHorizontalRuleNode(
  node: LexicalNode | null | undefined
): node is HorizontalRuleNode {
  return node instanceof HorizontalRuleNode;
}
